//! Asking a person to widen what may be changed.
//!
//! An approval obtained here is weaker evidence than one typed at a terminal,
//! and the design says so rather than pretending otherwise. The answer comes
//! back over the same pipe the request went out on, so the engine is trusting
//! that the far end is the client it handshook with. That is a reasonable
//! assumption for a stdio server the host launched, and it is an assumption.
//!
//! What follows from admitting that:
//!
//! - A client that did not declare the elicitation capability cannot be asked at
//!   all. The capability is recorded once, during initialize, and never inferred.
//! - A task may only collect so many approvals this way. Past that, further ones
//!   have to be given at a terminal, so a long run of small requests cannot
//!   quietly add up to a wide one.
//! - Every grant records how it was obtained, and the status output shows the
//!   count. A person who wants to know can find out.
//! - Nothing here can reach a protected path or a deny rule. Those are not a
//!   matter of permission, so they are refused before anybody is asked.

use std::time::{Duration, SystemTime};

use rmcp::RoleServer;
use rmcp::model::{ElicitRequestParams, ElicitationAction, ElicitationSchema};
use rmcp::service::Peer;

use crate::dataformatting::Msg;
use crate::domain::{OpSet, Operation};
use crate::error::{Denial, Error, ErrorCode, Result};
use crate::paths::RelPath;
use crate::policy::{ApprovalSource, Authority, EvaluationContext, Grant, evaluate};
use crate::session::WriteSession;

/// What is being asked for.
#[derive(Debug, Clone)]
pub struct ExpansionRequest {
    pub paths: Vec<RelPath>,
    pub operations: OpSet,
    /// Why the caller says it is needed. Shown to the person verbatim.
    pub reason: String,
}

/// Checks that the request is worth asking about, then asks.
///
/// Returns the grant if a person approved it. Everything else — already
/// allowed, never grantable, no way to ask, too many already — is a refusal
/// that never reaches anybody's screen.
pub async fn request_expansion(
    peer: &Peer<RoleServer>,
    session: &mut WriteSession,
    request: &ExpansionRequest,
) -> Result<Grant> {
    worth_asking(session, request)?;
    can_ask(session, request)?;

    let answer = peer
        .create_elicitation(ElicitRequestParams::FormElicitationParams {
            meta: None,
            message: prompt(request),
            // An empty schema: nothing is collected beyond the decision itself.
            // A free-text field would be something the engine has to interpret,
            // and the only question being asked is yes or no.
            requested_schema: ElicitationSchema::new(std::collections::BTreeMap::new()),
        })
        .await
        .map_err(|error| could_not_ask(&error))?;

    if answer.action != ElicitationAction::Accept {
        return Err(Error::Denied(Denial::new(
            ErrorCode::ApprovalRequired,
            Msg::ExpansionDeclined,
        )));
    }

    let minutes = session.policy().approval().grant_ttl_minutes;
    let client = session
        .client()
        .map_or_else(|| "unknown".to_owned(), |client| client.name.clone());

    let grant = Grant::new(
        session.task(),
        session.policy_version(),
        request.paths.clone(),
        request.operations,
        // Recorded, not collapsed into a boolean: an approval that arrived over
        // the client channel is weaker evidence than one typed at a terminal,
        // and the status output has to be able to say which it had.
        ApprovalSource::Elicitation {
            client,
            session: session.task().to_string(),
        },
        SystemTime::now() + Duration::from_secs(minutes * 60),
    );

    session.note_elicitation();
    session.admit_grant(&grant)?;
    Ok(grant)
}

/// Refuses requests that should never reach a person.
fn worth_asking(session: &WriteSession, request: &ExpansionRequest) -> Result<()> {
    let grants = session.grants()?;
    let context = EvaluationContext {
        grants: &grants,
        task: session.task(),
        policy_version: session.policy_version(),
        now: SystemTime::now(),
        authority: Authority::Requested,
    };

    for path in &request.paths {
        for operation in request.operations.iter() {
            let decision = evaluate(path, operation, session.policy(), &context);

            if decision.is_allowed() {
                return Err(Error::Denied(Denial::new(
                    ErrorCode::ScopeDenied,
                    Msg::ExpansionAlreadyAllowed {
                        path: path.as_str().to_owned(),
                    },
                )));
            }
            if !decision.may_request_expansion() {
                return Err(Error::Denied(Denial::new(
                    ErrorCode::ProtectedPath,
                    Msg::ExpansionCannotBeGranted {
                        path: path.as_str().to_owned(),
                    },
                )));
            }
        }
    }
    Ok(())
}

/// Refuses when there is no way to ask, or when too much has been asked already.
fn can_ask(session: &WriteSession, request: &ExpansionRequest) -> Result<()> {
    let at_a_terminal = Msg::HintApproveAtATerminal {
        paths: describe(request),
    };

    // A client that never declared it can ask a person cannot be asked. The
    // capability is recorded once, during initialize, and never inferred.
    if !session
        .client()
        .is_some_and(|client| client.can_ask_a_person)
    {
        return Err(Error::Denied(
            Denial::new(ErrorCode::ApprovalNeedsTty, Msg::ExpansionNeedsTerminal)
                .with_hint(at_a_terminal),
        ));
    }

    let limit = session.policy().approval().max_elicitations_per_task;
    let used = session.elicitations();
    if used >= limit {
        return Err(Error::Denied(
            Denial::new(
                ErrorCode::ApprovalNeedsTty,
                Msg::ExpansionLimitReached { used, limit },
            )
            .with_hint(at_a_terminal),
        ));
    }
    Ok(())
}

/// The paths as a person would type them after `safescope approve`.
fn describe(request: &ExpansionRequest) -> String {
    request
        .paths
        .iter()
        .map(|path| path.as_str().to_owned())
        .collect::<Vec<_>>()
        .join(" ")
}

fn prompt(request: &ExpansionRequest) -> String {
    Msg::ExpansionPrompt {
        paths: describe(request),
        operations: request.operations.to_string(),
        reason: request.reason.clone(),
    }
    .to_string()
}

/// The client could not be asked, whatever the reason.
///
/// A transport failure and a refusal to answer are the same thing here: no
/// approval was obtained, so nothing is opened.
fn could_not_ask(error: &rmcp::service::ServiceError) -> Error {
    Error::Denied(
        Denial::new(ErrorCode::ApprovalNeedsTty, Msg::ExpansionNeedsTerminal).with_hint(
            Msg::McpTransportFailed {
                reason: error.to_string(),
            },
        ),
    )
}

/// Parses operation names from the wire.
pub fn parse_operations(names: &[String]) -> Result<OpSet> {
    names
        .iter()
        .map(|name| {
            name.parse::<Operation>()
                .map_err(|reason| Error::Denied(Denial::new(ErrorCode::InvalidPath, reason)))
        })
        .collect::<Result<Vec<_>>>()
        .map(|operations| operations.into_iter().collect())
}
