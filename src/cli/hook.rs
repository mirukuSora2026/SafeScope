//! The `PreToolUse` hook.
//!
//! Claude Code runs this before an edit tool, hands it the pending call on stdin
//! and reads a decision from stdout. What it can honestly do is narrow:
//!
//! - `Write`, `Edit`, `NotebookEdit` carry a path, so they can be judged.
//! - `Bash` cannot. A shell command is not reliably readable — `sed -i`, a
//!   redirect, a script — so no attempt is made to parse one. Guessing would
//!   produce a check that looks like protection and is not.
//! - Anything else is left alone.
//!
//! The manifest matches every tool rather than just the edit tools, because a
//! narrow matcher decides in the manifest what this file should decide with the
//! policy in front of it. Seeing `Bash` is not the same as judging it: in the
//! default mode this still says nothing about one.
//!
//! A policy may instead ask for `mode = "allowlist"`, which refuses every tool
//! not named — including `Bash`, not by reading the command but by not being on
//! the list. That was measured to be the only form of denial that holds: given a
//! list of forbidden tools the agent moves to one that is not on it, and the set
//! of tools that can run a command is not knowable in advance. An allowlist has
//! no such gap, and costs the agent the ability to run anything at all.
//!
//! **A hook is not the boundary, and the allowlist does not change that.** Per the host's documented behaviour, a hook
//! that fails with any exit code other than 0 or 2 is non-blocking and the tool
//! call proceeds; a disabled hook never runs at all. So every check here is also
//! performed inside the engine. This exists to catch a mistake early and say
//! something useful about it, not to be the thing standing in the way.
//!
//! It runs on every tool call, so it opens no database and hashes nothing: the
//! approved policy and one `symlink_metadata` are the whole of its work.

use std::io::Read as _;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::dataformatting::Msg;
use crate::domain::Operation;
use crate::error::{Error, Result};
use crate::inspect::Inspector;
use crate::paths::RelPath;
use crate::policy::{Authority, CompiledPolicy, EnforcementMode, EvaluationContext, evaluate};
use crate::registry;
use crate::store::baseline_store::BaselineStore;
use crate::store::policy_store::PolicyStore;

/// The pending tool call, as the host sends it.
///
/// Unknown fields are accepted on purpose — the opposite of the policy file,
/// which rejects them. This wire format belongs to the host and will grow; a
/// hook that broke on a new field would disable itself at the worst moment.
#[derive(Debug, Deserialize)]
struct HookInput {
    /// Which event this is. Anything unrecognised is answered with silence.
    #[serde(default)]
    hook_event_name: Option<String>,
    #[serde(default)]
    cwd: Option<String>,
    #[serde(default)]
    tool_name: String,
    #[serde(default)]
    tool_input: serde_json::Value,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct HookOutput {
    hook_specific_output: PreToolUseDecision,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct PreToolUseDecision {
    hook_event_name: &'static str,
    permission_decision: &'static str,
    permission_decision_reason: String,
}

/// Reads a pending tool call and writes a decision, or stays silent.
///
/// Always exits 0. Exit 2 would block unconditionally, which is not wanted: a
/// refusal is expressed as a decision so the reason reaches Claude, and anything
/// this hook cannot judge must fall through to the host's normal permission
/// flow rather than being blocked by an error.
pub fn run() -> Result<i32> {
    let mut raw = String::new();
    std::io::stdin()
        .read_to_string(&mut raw)
        .map_err(Error::from)?;

    // A malformed payload is not this hook's business to adjudicate.
    let Ok(input) = serde_json::from_str::<HookInput>(&raw) else {
        return Ok(crate::cli::exit::OK);
    };

    match input.hook_event_name.as_deref() {
        Some("PreToolUse") => pre_tool_use(&input),
        Some("SessionStart") => session_start(&input),
        Some("Stop") => stop(&input),
        // An event this build does not handle is answered with silence rather
        // than a guess, because a guess here becomes a decision.
        _ => Ok(crate::cli::exit::OK),
    }
}

/// Judges a pending tool call.
fn pre_tool_use(input: &HookInput) -> Result<i32> {
    if let Some(reason) = decide(input) {
        let output = HookOutput {
            hook_specific_output: PreToolUseDecision {
                hook_event_name: "PreToolUse",
                permission_decision: "deny",
                permission_decision_reason: reason,
            },
        };
        println!("{}", serde_json::to_string(&output).expect("serialisable"));
    }
    Ok(crate::cli::exit::OK)
}

/// Tells a new session what it should know before trusting the workspace.
///
/// Silence when there is nothing to say. A line that appears every session is a
/// line nobody reads.
fn session_start(input: &HookInput) -> Result<i32> {
    ensure_baseline(input);

    let Some(notes) = concerns(input) else {
        return Ok(crate::cli::exit::OK);
    };

    // SessionStart takes additionalContext, which is what puts this in front of
    // Claude rather than only in the transcript.
    let output = serde_json::json!({
        "hookSpecificOutput": {
            "hookEventName": "SessionStart",
            "additionalContext": notes.join("\n"),
        }
    });
    println!("{output}");
    Ok(crate::cli::exit::OK)
}

/// Says at the end of a turn what was left unfinished.
fn stop(input: &HookInput) -> Result<i32> {
    let Some(notes) = concerns(input) else {
        return Ok(crate::cli::exit::OK);
    };

    // Stop does not take hookSpecificOutput.additionalContext; systemMessage is
    // what it honours.
    let output = serde_json::json!({ "systemMessage": notes.join("\n") });
    println!("{output}");
    Ok(crate::cli::exit::OK)
}

/// Takes a drift baseline at the start of a session, if there is not one.
///
/// Here rather than only when a task begins, because a task begins at the first
/// SafeScope call and an agent may change files long before it makes one — or
/// never make one at all, which is the case worth catching. A baseline taken
/// then would quietly adopt whatever had already been done as the starting
/// point, and report a clean workspace.
///
/// Takes no lock and does not need one: the baseline is written atomically, and
/// two sessions starting together would write the same answer. Failure is
/// silent, because a hook that fails loudly is a hook a person turns off.
fn ensure_baseline(input: &HookInput) {
    let Some(root) = input
        .cwd
        .as_ref()
        .map(PathBuf::from)
        .and_then(|cwd| workspace_root(&cwd))
    else {
        return;
    };
    let Ok(registration) = registry::load(&root) else {
        return;
    };
    let Ok(paths) = registration.state_paths() else {
        return;
    };
    let _ = BaselineStore::new(&paths).ensure(&root);
}

/// What is worth telling somebody about this workspace, if anything.
///
/// Through an Inspector, which takes no lock. A session takes the workspace
/// lock and the MCP server holds one for its whole life, so a hook that opened
/// a session would be a hook that never ran while the server was up.
fn concerns(input: &HookInput) -> Option<Vec<String>> {
    let cwd = input.cwd.as_ref().map(PathBuf::from)?;
    let root = workspace_root(&cwd)?;
    let notes = Inspector::open(&root).ok()?.concerns().ok()?;

    (!notes.is_empty()).then(|| notes.iter().map(ToString::to_string).collect())
}

/// Tools the allowlist permits without being named in a policy.
///
/// Reading, searching and asking questions: everything here leaves the
/// workspace as it found it. `ToolSearch` is on it because SafeScope's own MCP
/// tools may be deferred in a tool-heavy session, and an agent that cannot
/// search for them cannot reach the audited path either.
///
/// Deliberately absent: `Bash`, and the tools that hand work to something
/// holding its own — `Agent`, `Task`, `Skill`, `Monitor`. Those were the four
/// routes the agent actually took when the edit tools were denied.
const READ_ONLY_TOOLS: [&str; 8] = [
    "Read",
    "Glob",
    "Grep",
    "LS",
    "NotebookRead",
    "ToolSearch",
    "TodoWrite",
    "ExitPlanMode",
];

/// Whether a tool reaches SafeScope's own audited path.
fn is_safescope_tool(tool: &str) -> bool {
    tool.starts_with("mcp__safescope__")
}

/// Whether the allowlist admits this tool.
fn allowed_by_list(tool: &str, extra: &[String]) -> bool {
    is_safescope_tool(tool)
        || READ_ONLY_TOOLS.contains(&tool)
        || extra.iter().any(|allowed| allowed == tool)
}

/// The refusal reason, or `None` to say nothing.
///
/// Saying nothing is the common case and the right default: it leaves the host's
/// normal permission flow in charge. An explicit `allow` would quietly waive
/// whatever else the user had configured.
fn decide(input: &HookInput) -> Option<String> {
    let cwd = input.cwd.as_ref().map(PathBuf::from)?;
    let root = workspace_root(&cwd)?;
    let registration = registry::load(&root).ok()?;
    let store = PolicyStore::new(&registration.state_paths().ok()?);

    let approved = store.current().ok()?;

    // The allowlist, when a policy asks for one. Checked before the path, because
    // the tools it exists to refuse are the ones that have no path to check.
    if let Some(approved) = &approved
        && approved.policy.enforcement.mode == EnforcementMode::Allowlist
        && !allowed_by_list(&input.tool_name, &approved.policy.enforcement.allow_tools)
    {
        let mut allowed: Vec<&str> = READ_ONLY_TOOLS.to_vec();
        allowed.push("mcp__safescope__*");
        let extra: Vec<&str> = approved
            .policy
            .enforcement
            .allow_tools
            .iter()
            .map(String::as_str)
            .collect();
        allowed.extend(extra);
        return Some(format!(
            "{}\n{}",
            Msg::HookToolNotAllowed {
                tool: input.tool_name.clone(),
            },
            Msg::HintAllowlistMode {
                allowed: allowed.join(", "),
            }
        ));
    }

    let absolute = target_path(&input.tool_name, &input.tool_input)?;
    let relative = absolute.strip_prefix(&root).ok()?;
    // Outside the workspace, SafeScope has nothing to say.
    let path = RelPath::from_platform(relative).ok()?;

    let Some(approved) = approved else {
        // Registered but never approved: nothing may be changed yet, and saying
        // so is more useful than letting the write land and be unrecorded.
        return Some(
            Msg::HintFillInAllowThenApprove {
                policy: registration.policy_path().display().to_string(),
            }
            .to_string(),
        );
    };

    let compiled = CompiledPolicy::compile(approved.policy).ok()?;
    let operation = infer_operation(&absolute);
    let context = EvaluationContext {
        // Temporary approvals belong to a task, and a hook is not inside one.
        grants: &[],
        task: crate::ids::TaskId::new(),
        policy_version: approved.version,
        now: std::time::SystemTime::now(),
        authority: Authority::Requested,
    };

    match evaluate(&path, operation, &compiled, &context).into_result() {
        Ok(_) => None,
        Err(denial) => Some(match denial.hint() {
            Some(hint) => format!("{}\n{hint}", denial.message()),
            None => denial.message().to_owned(),
        }),
    }
}

/// The path a tool is about to write to, if it has one.
fn target_path(tool_name: &str, tool_input: &serde_json::Value) -> Option<PathBuf> {
    let field = match tool_name {
        "Write" | "Edit" | "MultiEdit" => "file_path",
        "NotebookEdit" => "notebook_path",
        // Bash and everything else: no path this hook can trust.
        _ => return None,
    };
    tool_input.get(field)?.as_str().map(PathBuf::from)
}

/// Walks up from `start` looking for a registered workspace.
fn workspace_root(start: &Path) -> Option<PathBuf> {
    start
        .ancestors()
        .find(|directory| {
            directory
                .join(registry::CONFIG_DIR)
                .join(registry::ID_FILE)
                .is_file()
        })
        .map(Path::to_path_buf)
}

/// Whether the pending write creates or replaces.
///
/// One `symlink_metadata` call, no hashing: this runs on every tool call, and a
/// large file would otherwise be read from disk before every edit.
fn infer_operation(absolute: &Path) -> Operation {
    match std::fs::symlink_metadata(absolute) {
        Ok(_) => Operation::Replace,
        Err(_) => Operation::Create,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn reads_the_path_from_edit_tools() {
        for tool in ["Write", "Edit", "MultiEdit"] {
            let input = json!({ "file_path": "/p/src/a.rs" });
            assert_eq!(
                target_path(tool, &input),
                Some(PathBuf::from("/p/src/a.rs")),
                "{tool}"
            );
        }
        assert_eq!(
            target_path("NotebookEdit", &json!({ "notebook_path": "/p/n.ipynb" })),
            Some(PathBuf::from("/p/n.ipynb"))
        );
    }

    #[test]
    fn declines_to_judge_a_shell_command() {
        // `sed -i`, a redirect, a script that writes: none of these can be read
        // reliably, and a check that looks like protection without being it is
        // worse than none.
        let input = json!({ "command": "sed -i s/a/b/ src/main.rs" });
        assert_eq!(target_path("Bash", &input), None);
    }

    #[test]
    fn ignores_tools_without_a_path() {
        assert_eq!(
            target_path("Read", &json!({ "file_path": "/p/a.rs" })),
            None
        );
        assert_eq!(target_path("Grep", &json!({})), None);
        assert_eq!(
            target_path("Write", &json!({})),
            None,
            "no path, no opinion"
        );
    }

    #[test]
    fn says_nothing_without_a_working_directory() {
        let input = HookInput {
            hook_event_name: Some("PreToolUse".to_owned()),
            cwd: None,
            tool_name: "Write".to_owned(),
            tool_input: json!({ "file_path": "/p/src/a.rs" }),
        };
        assert!(decide(&input).is_none());
    }

    #[test]
    fn says_nothing_outside_a_workspace() {
        let input = HookInput {
            hook_event_name: Some("PreToolUse".to_owned()),
            cwd: Some("/definitely/not/a/workspace".to_owned()),
            tool_name: "Write".to_owned(),
            tool_input: json!({ "file_path": "/definitely/not/a/workspace/a.rs" }),
        };
        assert!(decide(&input).is_none());
    }
}
