//! Asking a person to widen what may be changed.
//!
//! The elicitation goes out over the wire and the answer comes back over it, so
//! these drive a real server and answer as a real client would. A test that
//! called the approval function directly would be asserting that the author's
//! idea of a person's answer is accepted, which is not the property that matters.

mod mcp_client;

use mcp_client::{Answer, Client};
use serde_json::json;

/// The request the sample policy refuses but could be persuaded about.
fn outside_scope() -> serde_json::Value {
    json!({
        "paths": ["docs/Notes.md"],
        "operations": ["create"],
        "reason": "the fix needs a note recorded beside the docs",
    })
}

#[test]
fn an_approved_expansion_opens_exactly_what_was_asked_for() {
    let mut client = Client::connect();
    client.will_answer(Answer::Accept);

    let granted = client.call("request_scope_expansion", outside_scope());
    assert_eq!(granted["paths"], json!(["docs/Notes.md"]));
    assert_eq!(granted["operations"], json!(["create"]));
    assert_eq!(granted["approved_via"], "client");
    assert!(granted["expires_in_seconds"].as_u64().expect("ttl") > 0);
}

#[test]
fn a_granted_path_can_then_be_changed() {
    let mut client = Client::connect();
    std::fs::create_dir_all(client.workspace().join("docs")).expect("mkdir");

    // Refused before the approval.
    let error = client.call_expecting_refusal(
        "prepare_change",
        json!({ "operation": "create", "path": "docs/Notes.md", "contents": "x\n" }),
    );
    assert_eq!(error["data"]["code"], "SCOPE_DENIED");

    client.will_answer(Answer::Accept);
    client.call("request_scope_expansion", outside_scope());

    let prepared = client.call(
        "prepare_change",
        json!({ "operation": "create", "path": "docs/Notes.md", "contents": "noted\n" }),
    );
    client.call("apply_change", json!({ "plan_id": prepared["plan_id"] }));
    assert_eq!(client.read("docs/Notes.md"), "noted\n");
}

#[test]
fn a_declined_request_opens_nothing() {
    let mut client = Client::connect();
    client.will_answer(Answer::Decline);

    let error = client.call_expecting_refusal("request_scope_expansion", outside_scope());
    assert_eq!(error["data"]["code"], "APPROVAL_REQUIRED");

    // Still refused afterwards.
    let refusal = client.call_expecting_refusal(
        "prepare_change",
        json!({ "operation": "create", "path": "docs/Notes.md", "contents": "x\n" }),
    );
    assert_eq!(refusal["data"]["code"], "SCOPE_DENIED");
}

#[test]
fn a_cancelled_request_opens_nothing() {
    let mut client = Client::connect();
    client.will_answer(Answer::Cancel);

    let error = client.call_expecting_refusal("request_scope_expansion", outside_scope());
    assert_eq!(error["data"]["code"], "APPROVAL_REQUIRED");
}

#[test]
fn an_approval_covers_only_the_paths_it_named() {
    // Grants hold exact paths, never patterns: asked for one file, a client
    // cannot end up with the directory around it.
    let mut client = Client::connect();
    std::fs::create_dir_all(client.workspace().join("docs")).expect("mkdir");
    client.will_answer(Answer::Accept);
    client.call("request_scope_expansion", outside_scope());

    let error = client.call_expecting_refusal(
        "prepare_change",
        json!({ "operation": "create", "path": "docs/Other.md", "contents": "x\n" }),
    );
    assert_eq!(error["data"]["code"], "SCOPE_DENIED");
}

#[test]
fn an_approval_covers_only_the_operations_it_named() {
    let mut client = Client::connect();
    client.write("docs-file-stand-in.txt", "x\n");
    client.will_answer(Answer::Accept);
    client.call(
        "request_scope_expansion",
        json!({
            "paths": ["docs-file-stand-in.txt"],
            "operations": ["replace"],
            "reason": "only the contents need changing",
        }),
    );

    let error = client.call_expecting_refusal(
        "prepare_change",
        json!({ "operation": "trash", "path": "docs-file-stand-in.txt" }),
    );
    assert_eq!(error["data"]["code"], "SCOPE_DENIED");
}

#[test]
fn a_client_that_cannot_ask_is_told_to_use_a_terminal() {
    // The capability is recorded during initialize and never inferred, so a
    // client that did not declare it cannot be asked at all.
    let mut client = Client::connect_without_elicitation();

    let error = client.call_expecting_refusal("request_scope_expansion", outside_scope());
    assert_eq!(error["data"]["code"], "APPROVAL_NEEDS_TTY");

    let hint = error["data"]["hint"].as_str().expect("hint");
    assert!(hint.contains("safescope approve docs/Notes.md"), "{hint}");
}

#[test]
fn an_already_allowed_path_is_refused_without_asking_anybody() {
    // Nobody's attention is spent on a question with only one answer.
    let mut client = Client::connect();
    // No answer is configured: if the server asked, the client would panic.

    let error = client.call_expecting_refusal(
        "request_scope_expansion",
        json!({
            "paths": ["src/Already.java"],
            "operations": ["create"],
            "reason": "this is already in scope",
        }),
    );
    assert_eq!(error["data"]["code"], "SCOPE_DENIED");
}

#[test]
fn a_protected_path_is_refused_without_asking_anybody() {
    let mut client = Client::connect();

    let error = client.call_expecting_refusal(
        "request_scope_expansion",
        json!({
            "paths": [".git/config"],
            "operations": ["replace"],
            "reason": "please",
        }),
    );
    assert_eq!(error["data"]["code"], "PROTECTED_PATH");
}

#[test]
fn a_denied_path_is_refused_without_asking_anybody() {
    let mut client = Client::connect();

    let error = client.call_expecting_refusal(
        "request_scope_expansion",
        json!({
            "paths": ["src/.env"],
            "operations": ["replace"],
            "reason": "please",
        }),
    );
    assert_eq!(error["data"]["code"], "PROTECTED_PATH");
}

#[test]
fn a_run_of_requests_escalates_to_a_terminal() {
    // A long series of small approvals must not quietly add up to a wide one.
    let mut client = Client::connect();
    client.will_answer(Answer::Accept);

    for name in ["One", "Two", "Three"] {
        client.call(
            "request_scope_expansion",
            json!({
                "paths": [format!("docs/{name}.md")],
                "operations": ["create"],
                "reason": "another one",
            }),
        );
    }

    let error = client.call_expecting_refusal(
        "request_scope_expansion",
        json!({
            "paths": ["docs/Four.md"],
            "operations": ["create"],
            "reason": "one more",
        }),
    );
    assert_eq!(error["data"]["code"], "APPROVAL_NEEDS_TTY");
    assert!(
        error["message"]
            .as_str()
            .expect("message")
            .contains("3 of 3"),
        "{error}"
    );
}

#[test]
fn status_counts_the_approvals_in_force() {
    // A person who wants to know how permission was obtained can find out.
    let mut client = Client::connect();
    assert_eq!(
        client.call("get_status", json!({}))["temporary_approvals"],
        0
    );

    client.will_answer(Answer::Accept);
    client.call("request_scope_expansion", outside_scope());

    assert_eq!(
        client.call("get_status", json!({}))["temporary_approvals"],
        1
    );
}

#[test]
fn an_approval_from_another_process_is_seen_at_once() {
    // The whole reason grants live in a store rather than in memory. A person
    // typing `safescope approve` does so while the server is running and holding
    // the workspace lock, so the two processes have to see the same grant.
    use std::time::{Duration, SystemTime};

    use safescope::domain::OpSet;
    use safescope::paths::RelPath;
    use safescope::policy::{ApprovalSource, Grant};
    use safescope::store::grant_store::GrantStore;
    use safescope::store::policy_store::PolicyStore;
    use safescope::store::task_store::TaskStore;

    let mut client = Client::connect();
    std::fs::create_dir_all(client.workspace().join("docs")).expect("mkdir");

    let error = client.call_expecting_refusal(
        "prepare_change",
        json!({ "operation": "create", "path": "docs/Notes.md", "contents": "x\n" }),
    );
    assert_eq!(error["data"]["code"], "SCOPE_DENIED");

    // What `safescope approve` does once a person has confirmed.
    client.with_state(|paths| {
        let approved = PolicyStore::new(paths)
            .current()
            .expect("read")
            .expect("approved");
        let task = TaskStore::new(paths).current_or_start().expect("task");
        let grant = Grant::new(
            task,
            approved.version,
            vec![RelPath::parse("docs/Notes.md").expect("path")],
            OpSet::all(),
            ApprovalSource::Terminal,
            SystemTime::now() + Duration::from_secs(600),
        );
        GrantStore::new(paths).issue(&grant).expect("issue");
    });

    let prepared = client.call(
        "prepare_change",
        json!({ "operation": "create", "path": "docs/Notes.md", "contents": "noted\n" }),
    );
    client.call("apply_change", json!({ "plan_id": prepared["plan_id"] }));
    assert_eq!(client.read("docs/Notes.md"), "noted\n");

    // And the status says how that permission was obtained.
    assert_eq!(
        client.call("get_status", json!({}))["temporary_approvals"],
        1
    );
}
