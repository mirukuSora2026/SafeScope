//! Approving a policy.
//!
//! Approval is the moment a policy file becomes something the engine acts on, so
//! it is the one command that insists on a person.
//!
//! The terminal check is not a strong boundary and is not presented as one. An
//! agent with shell access can run this binary; what it cannot easily do is
//! answer an interactive prompt on a terminal it does not have. That raises the
//! cost of a silent self-approval without eliminating it, and the limitation
//! belongs in the documentation rather than in a claim of safety.

use std::io::{BufRead, IsTerminal, Write};
use std::path::Path;

use crate::dataformatting::{Label, Msg};
use crate::error::{Denial, Error, ErrorCode, Result};
use crate::policy::{NormalizedPolicy, PolicyDocument, ProtectedPaths, Severity, validate};
use crate::registry::{self, Registration};
use crate::store::policy_store::{ApprovedPolicy, PolicyStore};

/// Checks and approves the policy file, after asking the person to confirm.
pub fn run(workspace: &Path) -> Result<()> {
    let registration = registry::load(workspace)?;
    let text = registration.read_policy_text()?;
    let policy = check_policy(&text)?;

    if !std::io::stdin().is_terminal() {
        return Err(Error::Denied(
            Denial::new(
                ErrorCode::ApprovalNeedsTty,
                "Approving a policy has to be done by a person at a terminal.",
            )
            .with_hint("Run `safescope policy approve` in an interactive shell."),
        ));
    }

    summarise(&policy);
    if !confirmed(&mut std::io::stdin().lock(), &mut std::io::stdout())? {
        return Err(Error::Denied(Denial::new(
            ErrorCode::ApprovalRequired,
            "Not approved.",
        )));
    }

    let approved = perform(&registration, policy, &text)?;
    println!(
        "{}: {} {}",
        Msg::Label(Label::Approved),
        Msg::Label(Label::PolicyVersion),
        approved.version
    );
    Ok(())
}

/// Parses and validates the policy text, returning it desugared.
///
/// Every error is reported at once; fixing one per attempt wastes the person's
/// time.
pub fn check_policy(text: &str) -> Result<NormalizedPolicy> {
    let document = PolicyDocument::parse(text)?;
    let policy = NormalizedPolicy::from_document(&document);
    let report = validate(&document, &policy, &ProtectedPaths::engine_defaults());

    for warning in report.warnings() {
        eprintln!("{}: {}", Msg::Label(Label::Warnings), warning.message);
    }
    if let Some(denial) = report.to_denial() {
        return Err(Error::Denied(denial));
    }
    debug_assert!(
        report
            .diagnostics()
            .iter()
            .all(|d| d.severity == Severity::Warning)
    );
    Ok(policy)
}

/// Stores an approval. Separated from the prompt so it can be tested.
pub fn perform(
    registration: &Registration,
    policy: NormalizedPolicy,
    source_text: &str,
) -> Result<ApprovedPolicy> {
    let paths = registration.state_paths()?;
    paths.create()?;
    PolicyStore::new(&paths).approve(policy, source_text)
}

/// Shows what is approved, and whether the file has been edited since.
pub fn show(workspace: &Path) -> Result<()> {
    let registration = registry::load(workspace)?;
    let store = PolicyStore::new(&registration.state_paths()?);

    let Some(approved) = store.current()? else {
        println!(
            "{}",
            Msg::HintFillInAllowThenApprove {
                policy: registration.policy_path().display().to_string(),
            }
        );
        return Ok(());
    };

    println!("{}: {}", Msg::Label(Label::PolicyVersion), approved.version);
    print_scope(&approved.policy);

    // The case this exists for: someone edits the policy, does not approve it,
    // and believes the change is in force.
    if let Ok(text) = registration.read_policy_text()
        && !store.matches_source(&approved, &text)
    {
        println!("{}", Msg::Label(Label::UnapprovedEdits));
    }
    Ok(())
}

fn summarise(policy: &NormalizedPolicy) {
    print_scope(policy);
}

fn print_scope(policy: &NormalizedPolicy) {
    println!("{}:", Msg::Label(Label::CurrentScope));
    if policy.allow.is_empty() {
        println!("  {}", Msg::Label(Label::Nothing));
    }
    for entry in &policy.allow {
        println!("  {}  ({})", entry.pattern, entry.ops);
    }
}

/// Asks the person to type the word, so a stray keypress cannot approve a policy.
fn confirmed(input: &mut impl BufRead, output: &mut impl Write) -> Result<bool> {
    write!(output, "Type `approve` to confirm: ").map_err(Error::from)?;
    output.flush().map_err(Error::from)?;

    let mut answer = String::new();
    input.read_line(&mut answer).map_err(Error::from)?;
    Ok(answer.trim() == "approve")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_whole_word_confirms() {
        // A stray newline or a "y" must not approve a policy.
        for answer in ["approve\n", "  approve  \n"] {
            let mut output = Vec::new();
            assert!(confirmed(&mut answer.as_bytes(), &mut output).unwrap());
        }
        for answer in ["\n", "y\n", "yes\n", "APPROVE\n", "approved\n", ""] {
            let mut output = Vec::new();
            assert!(
                !confirmed(&mut answer.as_bytes(), &mut output).unwrap(),
                "{answer:?} should not confirm"
            );
        }
    }

    #[test]
    fn an_invalid_policy_is_refused_before_anything_is_stored() {
        let error = check_policy("schema_version = 1\n[scope]\nallow = []\n").unwrap_err();
        assert_eq!(error.code(), ErrorCode::PolicyInvalid);
    }

    #[test]
    fn a_usable_policy_passes_the_check() {
        let policy = check_policy("schema_version = 1\n[scope]\nallow = [\"src/**\"]\n").unwrap();
        assert_eq!(policy.allow.len(), 1);
    }
}
