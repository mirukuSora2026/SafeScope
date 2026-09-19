//! Explaining a decision without changing anything.
//!
//! This is the command to reach for when a refusal is puzzling, and it is the
//! whole deliverable of the policy engine: a person who cannot find out *why* a
//! path was refused will stop using the policy.
//!
//! The evaluation table is derived from the decision rather than recorded during
//! it. That works because the order is fixed — a decision from a deny rule means
//! the protected check ran and did not match — and it keeps the evaluator free of
//! reporting concerns.

use std::path::Path;
use std::time::SystemTime;

use crate::dataformatting::{Label, Msg, pad};

/// Terminal cells reserved for a row label.
const LABEL_WIDTH: usize = 14;
/// Terminal cells reserved for an evaluation-step label.
const STEP_WIDTH: usize = 26;
use crate::domain::Operation;
use crate::error::{Denial, Error, ErrorCode, Result};
use crate::ids::TaskId;
use crate::paths::RelPath;
use crate::policy::{
    Authority, CompiledPolicy, Decision, EvaluationContext, NormalizedPolicy, RuleSource, evaluate,
};
use crate::registry;
use crate::store::policy_store::PolicyStore;

/// Explains whether an operation on a path would be permitted.
pub fn run(workspace: &Path, path: &str, operation: &str) -> Result<i32> {
    let operation: Operation = operation
        .parse()
        .map_err(|reason: String| Error::Denied(Denial::new(ErrorCode::InvalidPath, reason)))?;
    let path = RelPath::parse(path)?;

    let registration = registry::load(workspace)?;
    let store = PolicyStore::new(&registration.state_paths()?);
    let Some(approved) = store.current()? else {
        return Err(Error::Denied(
            Denial::new(ErrorCode::NoApprovedPolicy, Msg::Label(Label::Nothing)).with_hint(
                Msg::HintFillInAllowThenApprove {
                    policy: registration.policy_path().display().to_string(),
                },
            ),
        ));
    };

    let compiled = CompiledPolicy::compile(approved.policy.clone())?;
    // `check` asks a standing question, so no task's temporary approvals apply.
    let context = EvaluationContext {
        grants: &[],
        task: TaskId::new(),
        policy_version: approved.version,
        now: SystemTime::now(),
        authority: Authority::Requested,
    };
    let decision = evaluate(&path, operation, &compiled, &context);

    print_decision(
        &decision,
        &path,
        operation,
        &approved.policy,
        approved.version,
    );
    // The explanation is the output; the exit code is what a script reads.
    Ok(if decision.is_allowed() {
        crate::cli::exit::OK
    } else {
        crate::cli::exit::DENIED
    })
}

fn print_decision(
    decision: &Decision,
    path: &RelPath,
    operation: Operation,
    policy: &NormalizedPolicy,
    version: crate::policy::PolicyVersion,
) {
    let headline = match decision {
        Decision::Allow { .. } => Label::Allowed,
        Decision::NotCovered { .. } => Label::NotCovered,
        Decision::Deny { .. } => Label::Refused,
    };
    println!("{}", Msg::Label(headline));
    if let Decision::Deny { denial, .. } | Decision::NotCovered { denial } = decision {
        println!("  {}", denial.code());
        println!("  {}", denial.message());
    }
    println!();
    println!(
        "  {}{path}",
        pad(&Msg::Label(Label::Path).to_string(), LABEL_WIDTH)
    );
    println!(
        "  {:<12}{operation}",
        Msg::Label(Label::Operation).to_string()
    );
    println!(
        "  {:<12}{version}",
        Msg::Label(Label::PolicyVersion).to_string()
    );
    println!();

    println!("  {}", Msg::Label(Label::EvaluationSteps));
    for (label, outcome) in steps(decision) {
        println!(
            "    {}{outcome}",
            pad(&Msg::Label(label).to_string(), STEP_WIDTH)
        );
    }
    println!();

    // Only worth a row when it says something the headline did not.
    if !decision.is_allowed() {
        let outcome = if decision.may_request_expansion() {
            Msg::Label(Label::ExpansionPossible)
        } else {
            Msg::Label(Label::ExpansionImpossible)
        };
        println!(
            "  {}{outcome}",
            pad(&Msg::Label(Label::Outcome).to_string(), LABEL_WIDTH)
        );
        println!();
    }
    println!("  {}", Msg::Label(Label::CurrentScope));
    if policy.allow.is_empty() {
        println!("    {}", Msg::Label(Label::Nothing));
    }
    for entry in &policy.allow {
        println!("    {}  ({})", entry.pattern, entry.ops);
    }
}

/// The evaluation table, reconstructed from the decision.
///
/// Sound because the order is fixed: a decision that came from a deny rule
/// implies the protected check ran and found nothing.
fn steps(decision: &Decision) -> Vec<(Label, String)> {
    let order = [
        Label::StepProtected,
        Label::StepDeny,
        Label::StepAllow,
        Label::StepGrant,
    ];
    let reached = match decision.rule().map(|rule| &rule.source) {
        Some(RuleSource::Protected(_)) => 0,
        Some(RuleSource::PolicyDeny) => 1,
        Some(RuleSource::PolicyAllow) => 2,
        Some(RuleSource::Grant(_)) => 3,
        // `check` always asks as a caller would, so a reversal never reaches here.
        Some(RuleSource::Reversal) => 3,
        None => order.len(),
    };

    let matched = decision.rule().map(|rule| {
        rule.pattern.clone().map_or_else(
            || Msg::Label(Label::Allowed).to_string(),
            |pattern| match rule.line {
                Some(line) => format!("{pattern}  (policy.toml:{line})"),
                None => pattern,
            },
        )
    });

    order
        .into_iter()
        .enumerate()
        .take(reached + 1)
        .map(|(index, label)| {
            let outcome = if index == reached {
                matched
                    .clone()
                    .unwrap_or_else(|| Msg::Label(Label::NoMatch).to_string())
            } else {
                Msg::Label(Label::NoMatch).to_string()
            };
            (label, outcome)
        })
        .collect()
}
