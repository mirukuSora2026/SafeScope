//! A workspace with the engine's stores beside it, shared by the execution tests.

#![allow(dead_code)]

use std::fs;
use std::time::SystemTime;

use safescope::executor::{Executor, RequestKey};
use safescope::ids::{TaskId, WorkspaceId};
use safescope::journal::Journal;
use safescope::path_guard::Workspace;
use safescope::paths::RelPath;
use safescope::planner::{ChangePlan, ChangeRequest, Planner};
use safescope::policy::{CompiledPolicy, EvaluationContext, NormalizedPolicy, PolicyVersion};
use safescope::store::content::ContentStore;
use safescope::store::{DATA_DIR_ENV, StatePaths};
use tempfile::TempDir;

/// The policy the execution tests run against.
const POLICY: &str = "\
schema_version = 1

[scope]
allow = [\"src/**\"]
deny = [\"**/.env\"]
";

/// Serialises the tests, because `SAFESCOPE_DATA_DIR` is process-wide.
pub fn env_lock() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

pub fn path(text: &str) -> RelPath {
    RelPath::parse(text).unwrap()
}

/// A workspace with `src/` and the engine's stores beside it.
pub struct Harness {
    _data: TempDir,
    pub root: TempDir,
    pub workspace: Workspace,
    pub policy: CompiledPolicy,
    pub snapshots: ContentStore,
    pub staging: ContentStore,
    pub journal: Journal,
    pub task: TaskId,
}

impl Harness {
    pub fn new() -> Self {
        let data = TempDir::new().expect("data");
        let root = TempDir::new().expect("workspace");
        unsafe { std::env::set_var(DATA_DIR_ENV, data.path()) };

        fs::create_dir_all(root.path().join("src/auth")).expect("create src");
        let paths =
            StatePaths::for_workspace(WorkspaceId::new(), root.path()).expect("state paths");
        paths.create().expect("create state");

        Self {
            workspace: Workspace::open(root.path()).expect("open workspace"),
            policy: CompiledPolicy::compile(NormalizedPolicy::from_text(POLICY).unwrap())
                .expect("compile"),
            snapshots: ContentStore::snapshots(&paths),
            staging: ContentStore::staging(&paths),
            journal: Journal::open(&paths).expect("journal"),
            task: TaskId::new(),
            _data: data,
            root,
        }
    }

    pub fn context(&self) -> EvaluationContext<'_> {
        EvaluationContext {
            grants: &[],
            task: self.task,
            policy_version: PolicyVersion::FIRST,
            now: SystemTime::now(),
        }
    }

    pub fn plan(&self, request: ChangeRequest) -> safescope::error::Result<ChangePlan> {
        Planner {
            workspace: &self.workspace,
            policy: &self.policy,
            snapshots: &self.snapshots,
            staging: &self.staging,
        }
        .plan(&request, &self.context())
    }

    pub fn apply(
        &mut self,
        plan: &ChangePlan,
        request: Option<RequestKey>,
    ) -> safescope::error::Result<safescope::journal::OperationRecord> {
        let snapshots = self.snapshots.clone();
        let staging = self.staging.clone();
        Executor {
            workspace: &self.workspace,
            journal: &mut self.journal,
            snapshots: &snapshots,
            staging: &staging,
        }
        .apply(plan, request)
    }

    pub fn run(&mut self, request: ChangeRequest) -> safescope::error::Result<()> {
        let plan = self.plan(request)?;
        self.apply(&plan, None).map(|_| ())
    }

    pub fn write(&self, relative: &str, contents: &str) {
        fs::write(self.root.path().join(relative), contents).expect("write");
    }

    pub fn read(&self, relative: &str) -> String {
        fs::read_to_string(self.root.path().join(relative)).expect("read")
    }

    pub fn exists(&self, relative: &str) -> bool {
        self.root.path().join(relative).exists()
    }
}
