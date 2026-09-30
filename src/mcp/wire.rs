//! What the MCP tools take and return.
//!
//! These types are the contract with the client, so they are kept plain: no
//! engine types leak through, and every field is something a person reading the
//! transcript could understand. Error codes travel as their stable strings, not
//! as numbers.

use serde::{Deserialize, Serialize};

use crate::dataformatting::Msg;
use crate::error::{Denial, Error, ErrorCode, Result};
use crate::journal::OperationRecord;
use crate::paths::RelPath;
use crate::planner::ChangeRequest;
use crate::session::SessionStatus;

/// What a change should do.
#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum RequestedOperation {
    Create,
    Replace,
    Move,
    Trash,
}

/// A change to check and stage.
#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
pub struct PrepareChange {
    /// One of create, replace, move, trash.
    pub operation: RequestedOperation,
    /// Path relative to the workspace root.
    pub path: String,
    /// The destination, for a move.
    #[serde(default)]
    pub to: Option<String>,
    /// The whole new contents, for create and replace.
    #[serde(default)]
    pub contents: Option<String>,
}

/// A prepared plan, ready to apply.
#[derive(Debug, Clone, Serialize, schemars::JsonSchema)]
pub struct PreparedPlan {
    /// Pass this to apply_change. It is the only thing apply takes, so the
    /// contents cannot change between deciding and doing.
    pub plan_id: String,
    pub operation: String,
    /// Every path the change affects, which is what the budget counts.
    pub paths: Vec<String>,
    /// How long the plan stays applicable. After that it is refused rather than
    /// re-checked, because a plan is a statement about a moment.
    pub expires_in_seconds: u64,
}

/// A plan to carry out.
#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
pub struct ApplyChange {
    pub plan_id: String,
    /// An idempotency key: any string you choose. Resending the same key with
    /// the same plan returns the original result rather than acting twice,
    /// which is what to do when an answer was lost.
    #[serde(default)]
    pub request_id: Option<String>,
}

/// What an applied operation did.
#[derive(Debug, Clone, Serialize, schemars::JsonSchema)]
pub struct AppliedChange {
    pub operation_id: String,
    pub stage: String,
    pub operation: String,
    pub paths: Vec<String>,
}

impl AppliedChange {
    pub fn of(record: &OperationRecord) -> Self {
        Self {
            operation_id: record.id.to_string(),
            stage: record.stage.to_string(),
            operation: record.transition.operation().to_string(),
            paths: record
                .transition
                .touched_paths()
                .into_iter()
                .map(|path| path.as_str().to_owned())
                .collect(),
        }
    }
}

/// How many past operations to return.
#[derive(Debug, Clone, Default, Deserialize, schemars::JsonSchema)]
pub struct HistoryQuery {
    /// Most recent first. Defaults to twenty.
    #[serde(default)]
    pub limit: Option<usize>,
}

/// Nothing. Present because a tool needs a parameter type.
#[derive(Debug, Clone, Default, Deserialize, schemars::JsonSchema)]
pub struct NoArguments {}

/// A prepared reversal.
#[derive(Debug, Clone, Serialize, schemars::JsonSchema)]
pub struct PreparedUndo {
    pub plan_id: String,
    /// The operation this would reverse.
    pub reverses: String,
    pub operation: String,
    pub paths: Vec<String>,
}

/// A reversal to carry out.
#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
pub struct ApplyUndo {
    pub plan_id: String,
}

/// One rule the policy allows.
#[derive(Debug, Clone, Serialize, schemars::JsonSchema)]
pub struct AllowedRule {
    pub pattern: String,
    pub operations: Vec<String>,
}

/// A limit and what has been spent against it.
#[derive(Debug, Clone, Serialize, schemars::JsonSchema)]
pub struct Consumption {
    pub used: u64,
    pub limit: u64,
}

/// What the session can say about itself.
#[derive(Debug, Clone, Serialize, schemars::JsonSchema)]
pub struct Status {
    pub task_id: String,
    pub policy_version: u64,
    pub allowed: Vec<AllowedRule>,
    pub changed_paths: Consumption,
    pub operations: Consumption,
    pub moves: Consumption,
    pub recovery_storage_bytes: Consumption,
    /// Operations that have not reached a settled stage.
    pub unsettled: usize,
    /// Of those, the ones a person has to compare before the workspace is
    /// trustworthy again.
    pub needs_attention: usize,
    /// True when the policy file has been edited but not approved. Whoever made
    /// that edit probably believes it is in force.
    pub unapproved_policy_edits: bool,
    /// Temporary approvals in force for this task.
    pub temporary_approvals: usize,
    /// What SafeScope does not cover, stated rather than implied.
    pub coverage: String,
}

impl Status {
    pub fn of(status: &SessionStatus, coverage: String) -> Self {
        Self {
            task_id: status.task.to_string(),
            policy_version: status.policy_version.get(),
            allowed: status
                .allowed
                .iter()
                .map(|entry| AllowedRule {
                    pattern: entry.pattern.clone(),
                    operations: entry
                        .ops
                        .iter()
                        .map(|operation| operation.to_string())
                        .collect(),
                })
                .collect(),
            changed_paths: Consumption {
                used: status.usage.paths(),
                limit: status.limits.max_changed_paths,
            },
            operations: Consumption {
                used: status.usage.operations,
                limit: status.limits.max_operations,
            },
            moves: Consumption {
                used: status.usage.moves,
                limit: status.limits.max_moves,
            },
            recovery_storage_bytes: Consumption {
                used: status.usage.snapshot_bytes,
                limit: status.limits.max_snapshot_bytes,
            },
            unsettled: status.unsettled,
            needs_attention: status.needs_attention,
            unapproved_policy_edits: status.unapproved_policy_edits,
            temporary_approvals: status.grants,
            coverage,
        }
    }
}

/// What `get_history` answers with.
///
/// A wrapper around the list rather than the list itself. A tool's
/// `outputSchema` describes structured content, which is a JSON object, so a
/// top-level array is not a schema a client can accept — and one that rejects
/// it drops every tool the server offers, not only this one.
#[derive(Debug, Clone, Serialize, schemars::JsonSchema)]
pub struct History {
    /// Most recent first.
    pub operations: Vec<HistoryEntry>,
}

/// One past operation.
#[derive(Debug, Clone, Serialize, schemars::JsonSchema)]
pub struct HistoryEntry {
    pub operation_id: String,
    pub sequence: u64,
    pub kind: String,
    pub stage: String,
    pub operation: String,
    pub paths: Vec<String>,
    /// Set when the operation could not be settled cleanly.
    pub error_code: Option<String>,
}

impl HistoryEntry {
    pub fn of(record: &OperationRecord) -> Self {
        Self {
            operation_id: record.id.to_string(),
            sequence: record.sequence,
            kind: record.kind.to_string(),
            stage: record.stage.to_string(),
            operation: record.transition.operation().to_string(),
            paths: record
                .transition
                .touched_paths()
                .into_iter()
                .map(|path| path.as_str().to_owned())
                .collect(),
            error_code: record.error_code.clone(),
        }
    }
}

/// Turns an engine error into something the client can act on.
///
/// The stable code and the hint both travel. The hint is what stops a client
/// retrying a request that can never succeed, so losing it here would undo the
/// work every denial does to carry one.
pub fn to_mcp_error(error: &Error) -> rmcp::ErrorData {
    let report = error.report();
    let data = serde_json::json!({
        "code": report.code,
        "kind": report.kind,
        "retryable": report.retryable,
        "hint": report.hint,
    });

    let message = match &report.hint {
        Some(hint) => format!("{}: {}\n{hint}", report.code, report.message),
        None => format!("{}: {}", report.code, report.message),
    };

    if error.is_denial() {
        rmcp::ErrorData::invalid_params(message, Some(data))
    } else {
        rmcp::ErrorData::internal_error(message, Some(data))
    }
}

/// A request to widen what may be changed.
#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
pub struct RequestExpansion {
    /// Exact paths, relative to the workspace root. Never patterns: this opens
    /// what was asked for and nothing beside it.
    pub paths: Vec<String>,
    /// Which operations to ask for. Empty means all of them.
    #[serde(default)]
    pub operations: Vec<String>,
    /// Why it is needed. Shown to the person verbatim, so it should read as an
    /// explanation rather than a label.
    pub reason: String,
}

/// What an approval opened.
#[derive(Debug, Clone, Serialize, schemars::JsonSchema)]
pub struct GrantedExpansion {
    pub paths: Vec<String>,
    pub operations: Vec<String>,
    /// How long the approval lasts. It covers this task only.
    pub expires_in_seconds: u64,
    /// How the approval was obtained, so the transcript records which it was.
    pub approved_via: String,
}

/// Turns a `prepare_change` request into the engine's own request type.
///
/// Here rather than beside the tool that receives one: this is the translation
/// between what a client sends and what the engine takes, which is the whole
/// subject of this module. `RelPath::parse` is the only way a path gets in, so
/// a traversing or absolute path is refused before the engine sees it.
pub fn to_change_request(request: &PrepareChange) -> Result<ChangeRequest> {
    let path = RelPath::parse(&request.path)?;
    Ok(match request.operation {
        RequestedOperation::Create => ChangeRequest::Create {
            path,
            contents: required_contents(request)?,
        },
        RequestedOperation::Replace => ChangeRequest::Replace {
            path,
            contents: required_contents(request)?,
        },
        RequestedOperation::Trash => ChangeRequest::Trash { path },
        RequestedOperation::Move => {
            let destination = request.to.as_deref().ok_or_else(|| missing("move", "to"))?;
            ChangeRequest::Move {
                from: path,
                to: RelPath::parse(destination)?,
            }
        }
    })
}

/// The contents a create or replace must carry.
fn required_contents(request: &PrepareChange) -> Result<Vec<u8>> {
    let operation = match request.operation {
        RequestedOperation::Create => "create",
        _ => "replace",
    };
    request
        .contents
        .as_ref()
        .map(|text| text.as_bytes().to_vec())
        .ok_or_else(|| missing(operation, "contents"))
}

/// A field the request needed and did not carry.
///
/// The hint names the operation as well as the field, because the failure this
/// keeps producing is a near miss rather than an omission: a real session sent
/// `content` for `contents`, read "missing field", and had nothing telling it
/// which name the operation actually wanted.
fn missing(operation: &str, field: &str) -> Error {
    Error::Denied(
        Denial::new(
            ErrorCode::InvalidPath,
            Msg::McpMissingField {
                field: field.to_owned(),
            },
        )
        .with_hint(Msg::HintWhatTheOperationNeeds {
            operation: operation.to_owned(),
            field: field.to_owned(),
        }),
    )
}

/// Names a request by the text the client chose for it.
///
/// Any string. The schema calls it an idempotency key and says nothing about
/// its shape, so a client sends `req-1` — which was once refused with "this
/// operation needs a request_id and none was given", a sentence that was false
/// and a reason to try again. Text that already reads as a request id is taken
/// as one; anything else is named by its hash, so the same text always names
/// the same request.
pub fn request_id_of(text: &str) -> Result<crate::ids::RequestId> {
    use std::str::FromStr as _;

    if text.trim().is_empty() {
        return Err(Error::Denied(Denial::new(
            ErrorCode::InvalidPath,
            Msg::McpMissingField {
                field: "request_id".to_owned(),
            },
        )));
    }
    Ok(crate::ids::RequestId::from_str(text).unwrap_or_else(|_| {
        let hash = crate::hash::ContentHash::of_bytes(text.as_bytes());
        let mut bytes = [0; 16];
        bytes.copy_from_slice(&hash.as_bytes()[..16]);
        crate::ids::RequestId::from_uuid(uuid::Uuid::from_bytes(bytes))
    }))
}

/// What binds a request to the plan it was sent with.
///
/// It covers the plan, so reusing a key for a different change is a mismatch
/// the journal refuses rather than a silent second application.
pub fn request_digest(plan: &crate::planner::ChangePlan) -> crate::hash::ContentHash {
    crate::hash::ContentHash::of_bytes(format!("{}:{:?}", plan.id, plan.transition).as_bytes())
}
