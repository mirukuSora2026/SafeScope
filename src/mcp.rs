//! The MCP server Claude talks to.
//!
//! Six tools, split deliberately into deciding and doing. `prepare_change`
//! checks a request and stages everything it needs; `apply_change` takes a plan
//! id and nothing else. That is not ceremony — it means the contents cannot be
//! substituted between the moment a change was judged and the moment it happens,
//! and the client has no way to ask for something other than what was approved.
//!
//! The server holds one [`WriteSession`] for its lifetime, which holds the
//! workspace lock. Two SafeScope servers on the same workspace is not a
//! situation the engine has to reason about, because the second one will not
//! start.

pub mod approval;
pub mod schema;
#[cfg(unix)]
pub mod socket;
pub mod wire;

use std::collections::HashMap;
use std::future::Future;
use std::path::PathBuf;
use std::str::FromStr as _;
use std::sync::Mutex;

use rmcp::handler::server::router::tool::ToolRouter;
use rmcp::handler::server::wrapper::{Json, Parameters};
use rmcp::model::{Implementation, InitializeRequestParams, InitializeResult, ServerCapabilities};
use rmcp::service::RequestContext;
use rmcp::{ErrorData, RoleServer, ServerHandler, tool, tool_handler, tool_router};

use crate::dataformatting::Msg;
use crate::error::Result;
use crate::ids::{PlanId, RequestId};
use crate::paths::RelPath;
use crate::planner::ChangePlan;
use crate::session::WriteSession;
use crate::undo::UndoPlan;

use self::wire::{
    AppliedChange, ApplyChange, ApplyUndo, GrantedExpansion, History, HistoryEntry, HistoryQuery,
    NoArguments, PrepareChange, PreparedPlan, PreparedUndo, RequestExpansion, Status,
    to_change_request, to_mcp_error,
};

/// How many past operations `get_history` returns by default.
const DEFAULT_HISTORY: usize = 20;

/// The server, and everything it is holding open.
pub struct SafeScope {
    /// The project this server speaks for.
    root: PathBuf,
    /// Opened on first use, not at startup.
    ///
    /// A workspace with no approved policy is a workspace SafeScope permits
    /// nothing in — but refusing to start over it would leave Claude unable to
    /// tell SafeScope apart from not being installed, and quietly falling back
    /// to whatever else it has. Starting and saying what is wrong is the more
    /// useful failure.
    ///
    /// A tokio mutex rather than a std one: the approval flow holds this across
    /// the question it puts to a person, and the session's view of the policy
    /// and the budget has to still be true when the answer arrives. Releasing it
    /// to await would reopen exactly the window the session exists to close.
    session: tokio::sync::Mutex<Option<WriteSession>>,
    /// Recorded during initialize, applied when the session opens.
    client: Mutex<Option<(String, bool)>>,
    /// Plans waiting to be applied.
    ///
    /// Held here rather than passed back and forth, so `apply_change` receives
    /// an identifier and cannot be handed different contents than the ones that
    /// were checked. Losing them to a restart costs nothing: the payload is
    /// already staged and the plan can be built again.
    plans: Mutex<HashMap<PlanId, ChangePlan>>,
    undos: Mutex<HashMap<PlanId, UndoPlan>>,
    tool_router: ToolRouter<Self>,
}

/// The router with every schema resolved into standing on its own.
///
/// See [`schema::flatten`]: a tool whose parameters are described by `$ref`
/// costs this server every tool it offers, not just that one.
fn flattened_router() -> ToolRouter<SafeScope> {
    let mut router = SafeScope::tool_router();
    for route in router.map.values_mut() {
        schema::flatten(&mut route.attr);
    }
    router
}

#[tool_router]
impl SafeScope {
    /// Prepares a server for `root`. The workspace is opened on first use.
    pub fn open(root: &std::path::Path) -> Result<Self> {
        Ok(Self {
            root: root.to_path_buf(),
            session: tokio::sync::Mutex::new(None),
            client: Mutex::new(None),
            plans: Mutex::new(HashMap::new()),
            undos: Mutex::new(HashMap::new()),
            tool_router: flattened_router(),
        })
    }

    /// The session, opening it if this is the first call.
    ///
    /// Every failure to open — not registered, no approved policy, another
    /// process holding the lock — reaches the caller as a tool error with its
    /// code and its hint, rather than as a server that never appeared.
    async fn session(
        &self,
    ) -> std::result::Result<tokio::sync::MappedMutexGuard<'_, WriteSession>, ErrorData> {
        let mut slot = self.session.lock().await;
        if slot.is_none() {
            let mut session =
                WriteSession::open(&self.root).map_err(|error| to_mcp_error(&error))?;
            if let Some((name, can_ask)) = self.client.lock().expect("client lock").clone() {
                session.note_client(name, can_ask);
            }
            *slot = Some(session);
        }
        Ok(tokio::sync::MutexGuard::map(slot, |slot| {
            slot.as_mut().expect("opened just above")
        }))
    }

    #[tool(
        name = "prepare_change",
        description = "Check a file change against the approved scope and budget and stage \
                       everything it needs. Changes nothing in the workspace. Returns a plan \
                       id to pass to apply_change."
    )]
    async fn prepare_change(
        &self,
        Parameters(request): Parameters<PrepareChange>,
    ) -> std::result::Result<Json<PreparedPlan>, ErrorData> {
        let change = to_change_request(&request).map_err(|error| to_mcp_error(&error))?;

        let session = self.session().await?;
        let plan = session
            .plan(&change)
            .map_err(|error| to_mcp_error(&error))?;
        drop(session);

        let prepared = PreparedPlan {
            plan_id: plan.id.to_string(),
            operation: plan.transition.operation().to_string(),
            paths: plan
                .transition
                .touched_paths()
                .into_iter()
                .map(|path| path.as_str().to_owned())
                .collect(),
            expires_in_seconds: seconds_until(plan.expires_at),
        };
        self.plans.lock().expect("plan lock").insert(plan.id, plan);
        Ok(Json(prepared))
    }

    #[tool(
        name = "apply_change",
        description = "Carry out a plan from prepare_change. Takes the plan id and nothing \
                       else, so the change that happens is the change that was checked."
    )]
    async fn apply_change(
        &self,
        Parameters(request): Parameters<ApplyChange>,
    ) -> std::result::Result<Json<AppliedChange>, ErrorData> {
        let id = parse_plan_id(&request.plan_id)?;
        let plan = self
            .plans
            .lock()
            .expect("plan lock")
            .get(&id)
            .cloned()
            .ok_or_else(|| unknown_plan(&request.plan_id))?;

        let key = request
            .request_id
            .as_deref()
            .map(|text| idempotency_key(text, &plan))
            .transpose()?;

        let mut session = self.session().await?;
        let record = session
            .apply(&plan, key)
            .map_err(|error| to_mcp_error(&error))?;

        // A spent plan is removed, so a second apply cannot reach for it. The
        // journal, not this map, is what makes a resend safe.
        self.plans.lock().expect("plan lock").remove(&id);
        Ok(Json(AppliedChange::of(&record)))
    }

    #[tool(
        name = "request_scope_expansion",
        description = "Ask the person to open exact paths that the policy does not currently \
                       allow. Only for a refusal that said an expansion could change the \
                       answer; a protected path or a deny rule is refused without anybody \
                       being asked. The approval covers this task only, for a limited time, \
                       and does not change the policy."
    )]
    async fn request_scope_expansion(
        &self,
        Parameters(request): Parameters<RequestExpansion>,
        context: RequestContext<RoleServer>,
    ) -> std::result::Result<Json<GrantedExpansion>, ErrorData> {
        let paths = request
            .paths
            .iter()
            .map(|text| RelPath::parse(text))
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(|denial| to_mcp_error(&crate::error::Error::Denied(denial)))?;
        let operations = if request.operations.is_empty() {
            crate::domain::OpSet::all()
        } else {
            approval::parse_operations(&request.operations).map_err(|error| to_mcp_error(&error))?
        };

        let expansion = approval::ExpansionRequest {
            paths,
            operations,
            reason: request.reason,
        };

        // The lock is held across the question, which can take as long as a
        // person takes. That is deliberate: the session's view of the policy and
        // the budget has to still be true when the answer arrives.
        let mut session = self.session().await?;
        let grant = approval::request_expansion(&context.peer, &mut session, &expansion)
            .await
            .map_err(|error| to_mcp_error(&error))?;

        Ok(Json(GrantedExpansion {
            paths: grant
                .paths
                .iter()
                .map(|path| path.as_str().to_owned())
                .collect(),
            operations: grant
                .ops
                .iter()
                .map(|operation| operation.to_string())
                .collect(),
            expires_in_seconds: seconds_until(grant.expires_at),
            approved_via: if grant.issued_by.is_terminal() {
                "terminal".to_owned()
            } else {
                "client".to_owned()
            },
        }))
    }

    #[tool(
        name = "get_status",
        description = "What this task has changed, what is left in the budget, and what \
                       SafeScope does not cover."
    )]
    async fn get_status(
        &self,
        Parameters(_): Parameters<NoArguments>,
    ) -> std::result::Result<Json<Status>, ErrorData> {
        let session = self.session().await?;
        let status = session.status().map_err(|error| to_mcp_error(&error))?;
        Ok(Json(Status::of(
            &status,
            Msg::McpCoverageNotice.to_string(),
        )))
    }

    #[tool(
        name = "get_history",
        description = "The operations this task has performed, most recent first, with what \
                       became of each."
    )]
    async fn get_history(
        &self,
        Parameters(query): Parameters<HistoryQuery>,
    ) -> std::result::Result<Json<History>, ErrorData> {
        let session = self.session().await?;
        let mut history = session
            .journal()
            .history(session.task())
            .map_err(|error| to_mcp_error(&error))?;
        history.reverse();
        history.truncate(query.limit.unwrap_or(DEFAULT_HISTORY));

        Ok(Json(History {
            operations: history.iter().map(HistoryEntry::of).collect(),
        }))
    }

    #[tool(
        name = "prepare_undo",
        description = "Work out how to reverse the most recent completed operation. Changes \
                       nothing. Refuses if the file has been edited since, rather than \
                       overwriting that edit."
    )]
    async fn prepare_undo(
        &self,
        Parameters(_): Parameters<NoArguments>,
    ) -> std::result::Result<Json<PreparedUndo>, ErrorData> {
        let session = self.session().await?;
        let undo = session
            .prepare_undo()
            .map_err(|error| to_mcp_error(&error))?;
        drop(session);

        let prepared = PreparedUndo {
            plan_id: undo.plan.id.to_string(),
            reverses: undo.reverses.to_string(),
            operation: undo.plan.transition.operation().to_string(),
            paths: undo
                .plan
                .transition
                .touched_paths()
                .into_iter()
                .map(|path| path.as_str().to_owned())
                .collect(),
        };
        self.undos
            .lock()
            .expect("undo lock")
            .insert(undo.plan.id, undo);
        Ok(Json(prepared))
    }

    #[tool(
        name = "apply_undo",
        description = "Carry out a reversal from prepare_undo. Does not spend the change \
                       budget."
    )]
    async fn apply_undo(
        &self,
        Parameters(request): Parameters<ApplyUndo>,
    ) -> std::result::Result<Json<AppliedChange>, ErrorData> {
        let id = parse_plan_id(&request.plan_id)?;
        let undo = self
            .undos
            .lock()
            .expect("undo lock")
            .get(&id)
            .cloned()
            .ok_or_else(|| unknown_plan(&request.plan_id))?;

        let mut session = self.session().await?;
        let record = session
            .apply_undo(&undo)
            .map_err(|error| to_mcp_error(&error))?;

        self.undos.lock().expect("undo lock").remove(&id);
        Ok(Json(AppliedChange::of(&record)))
    }
}

// Points the generated dispatch at the cached router. The default builds a
// fresh one on every call, which is work per request for a table that never
// changes.
#[tool_handler(router = self.tool_router)]
impl ServerHandler for SafeScope {
    fn get_info(&self) -> InitializeResult {
        // Both of these are #[non_exhaustive], so they are built from their
        // defaults rather than with a struct expression — which also means a new
        // field in a future rmcp keeps whatever that version considers sensible.
        let mut server_info = Implementation::default();
        server_info.name = "safescope".into();
        server_info.version = env!("CARGO_PKG_VERSION").into();

        let mut info = InitializeResult::default();
        info.capabilities = ServerCapabilities::builder().enable_tools().build();
        info.server_info = server_info;
        info.instructions = Some(Msg::McpInstructions.to_string());
        info
    }

    fn initialize(
        &self,
        request: InitializeRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> impl Future<Output = std::result::Result<InitializeResult, ErrorData>> + Send + '_ {
        // Whether the client can put a question to a person decides which
        // approvals the engine may accept later. It is recorded here, at the one
        // moment the client declares it, and never inferred afterwards.
        let supports_elicitation = request.capabilities.elicitation.is_some();
        let client = request.client_info.name.clone();
        async move {
            // Recorded even if the workspace cannot be opened, so it is in place
            // when it can be. Initialize must not fail over a workspace the
            // person may be about to set up.
            *self.client.lock().expect("client lock") = Some((client, supports_elicitation));
            if let Ok(mut session) = self.session().await {
                let recorded = self.client.lock().expect("client lock").clone();
                if let Some((name, can_ask)) = recorded {
                    session.note_client(name, can_ask);
                }
            }
            Ok(self.get_info())
        }
    }
}

/// Runs the server over stdio until the client disconnects.
///
/// Under a guard this relays instead. The engine has to be outside the sandbox
/// to carry out the changes it approves, so what the host starts here is a pipe
/// to it rather than a second copy of it.
pub async fn serve(root: PathBuf) -> Result<()> {
    #[cfg(unix)]
    if let Some(path) = socket::socket_from_env() {
        return socket::relay(&path).await;
    }
    let server = SafeScope::open(&root)?;
    let running = rmcp::ServiceExt::serve(server, rmcp::transport::stdio())
        .await
        .map_err(|error| {
            crate::error::Error::Faulted(crate::error::Fault::new(
                crate::error::ErrorCode::IoFailed,
                Msg::McpTransportFailed {
                    reason: error.to_string(),
                },
            ))
        })?;
    let _ = running.waiting().await;
    Ok(())
}

fn parse_plan_id(text: &str) -> std::result::Result<PlanId, ErrorData> {
    PlanId::from_str(text).map_err(|_| unknown_plan(text))
}

fn unknown_plan(text: &str) -> ErrorData {
    to_mcp_error(&crate::error::Error::Denied(
        crate::error::Denial::new(
            crate::error::ErrorCode::PlanNotFound,
            Msg::McpUnknownPlan {
                plan: text.to_owned(),
            },
        )
        .with_hint(Msg::HintRebuildThePlan),
    ))
}

/// Binds an idempotency key to the plan it was sent with.
///
/// The digest covers the plan, so reusing a key for a different change is a
/// mismatch the journal will refuse rather than a silent second application.
fn idempotency_key(
    text: &str,
    plan: &ChangePlan,
) -> std::result::Result<crate::executor::RequestKey, ErrorData> {
    let id = RequestId::from_str(text).map_err(|_| {
        to_mcp_error(&crate::error::Error::Denied(crate::error::Denial::new(
            crate::error::ErrorCode::InvalidPath,
            Msg::McpMissingField {
                field: "request_id".to_owned(),
            },
        )))
    })?;
    let digest =
        crate::hash::ContentHash::of_bytes(format!("{}:{:?}", plan.id, plan.transition).as_bytes());
    Ok(crate::executor::RequestKey { id, digest })
}

/// Seconds from now until `time`, or zero if it has passed.
fn seconds_until(time: std::time::SystemTime) -> u64 {
    time.duration_since(std::time::SystemTime::now())
        .map_or(0, |remaining| remaining.as_secs())
}
