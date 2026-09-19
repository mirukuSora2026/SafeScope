//! A binary that can be killed part-way through an operation.
//!
//! Crash recovery cannot be tested from inside a process: the point is what
//! survives when the process does not. This harness performs one real operation
//! on a real workspace so a test can abort it at a chosen fault point and then
//! ask recovery what it concludes.
//!
//! Built only with `--features fault-injection`, so it never ships.
//!
//! ```text
//! sfs-crash-harness setup <workspace>
//! SAFESCOPE_FAULT=after_applying_record sfs-crash-harness apply <workspace> replace
//! ```

use std::path::Path;
use std::process::ExitCode;

use safescope::cli::approve;
use safescope::error::Result;
use safescope::executor::Executor;
use safescope::journal::Journal;
use safescope::path_guard::Workspace;
use safescope::paths::RelPath;
use safescope::planner::{ChangeRequest, Planner};
use safescope::policy::{Authority, CompiledPolicy, EvaluationContext, PolicyVersion};
use safescope::registry;
use safescope::store::content::ContentStore;
use safescope::store::policy_store::PolicyStore;

/// Everything the harness touches is fixed, so a test knows what to expect.
const POLICY: &str = "schema_version = 1\n[scope]\nallow = [\"src/**\"]\n";
const TARGET: &str = "src/target.txt";
const DESTINATION: &str = "src/moved.txt";
pub const ORIGINAL: &[u8] = b"the original contents\n";
pub const REPLACEMENT: &[u8] = b"the replacement contents\n";
pub const CREATED: &[u8] = b"freshly created\n";
const CREATE_TARGET: &str = "src/created.txt";

/// The task every run shares, so recovery sees one task's history.
const TASK: &str = "00000000-0000-4000-8000-000000000001";

fn main() -> ExitCode {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    let outcome = match arguments.as_slice() {
        [mode, workspace] if mode == "setup" => setup(Path::new(workspace)),
        [mode, workspace, operation] if mode == "apply" => apply(Path::new(workspace), operation),
        _ => {
            eprintln!("usage: sfs-crash-harness setup|apply <workspace> [operation]");
            return ExitCode::from(64);
        }
    };

    match outcome {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{}: {}", error.report().code, error.report().message);
            ExitCode::FAILURE
        }
    }
}

/// Registers the workspace, approves a policy and lays down the target file.
fn setup(root: &Path) -> Result<()> {
    std::fs::create_dir_all(root.join("src")).expect("create src");
    let registration = registry::init(root)?;
    std::fs::write(registration.policy_path(), POLICY).expect("write policy");

    let policy = approve::check_policy(POLICY)?;
    approve::perform(&registration, policy, POLICY)?;

    std::fs::write(root.join(TARGET), ORIGINAL).expect("write target");
    Ok(())
}

/// Performs one operation. Aborts if a fault point is armed.
fn apply(root: &Path, operation: &str) -> Result<()> {
    let registration = registry::load(root)?;
    let paths = registration.state_paths()?;
    let approved = PolicyStore::new(&paths)
        .current()?
        .expect("the harness approves a policy during setup");

    let workspace = Workspace::open(root)?;
    let policy = CompiledPolicy::compile(approved.policy)?;
    let snapshots = ContentStore::snapshots(&paths);
    let staging = ContentStore::staging(&paths);
    let mut journal = Journal::open(&paths)?;

    let task = task_id();
    let context = EvaluationContext {
        grants: &[],
        task,
        policy_version: PolicyVersion::FIRST,
        now: std::time::SystemTime::now(),
        authority: Authority::Requested,
    };

    let request = match operation {
        "create" => ChangeRequest::Create {
            path: relative(CREATE_TARGET),
            contents: CREATED.to_vec(),
        },
        "replace" => ChangeRequest::Replace {
            path: relative(TARGET),
            contents: REPLACEMENT.to_vec(),
        },
        "trash" => ChangeRequest::Trash {
            path: relative(TARGET),
        },
        "move" => ChangeRequest::Move {
            from: relative(TARGET),
            to: relative(DESTINATION),
        },
        other => panic!("unknown operation {other:?}"),
    };

    let plan = Planner {
        workspace: &workspace,
        policy: &policy,
        journal: &journal,
        snapshots: &snapshots,
        staging: &staging,
    }
    .plan(&request, &context)?;

    let limits = *policy.budget();
    Executor {
        workspace: &workspace,
        journal: &mut journal,
        limits: &limits,
        snapshots: &snapshots,
        staging: &staging,
    }
    .apply(&plan, None)
    .map(|_| ())
}

fn relative(text: &str) -> RelPath {
    RelPath::parse(text).expect("a fixed path the harness controls")
}

/// The one task id every invocation uses, so recovery sees one history.
fn task_id() -> safescope::ids::TaskId {
    TASK.parse().expect("a fixed, well-formed uuid")
}
