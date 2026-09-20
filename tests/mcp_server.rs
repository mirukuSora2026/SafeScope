//! The MCP server, driven the way a client drives it.
//!
//! Real JSON-RPC over the real binary's stdio. Calling the tool functions
//! directly would test the author's idea of the protocol rather than the
//! protocol, and the wire shape is what a client actually depends on.

mod mcp_client;

use mcp_client::{Client, POLICY};
use serde_json::json;

#[test]
fn the_server_offers_its_whole_surface() {
    let mut client = Client::connect();
    let response = client.request("tools/list", json!({}));

    let mut names: Vec<&str> = response["result"]["tools"]
        .as_array()
        .expect("tools")
        .iter()
        .map(|tool| tool["name"].as_str().expect("name"))
        .collect();
    names.sort_unstable();

    assert_eq!(
        names,
        [
            "apply_change",
            "apply_undo",
            "get_history",
            "get_status",
            "prepare_change",
            "prepare_undo",
            "request_scope_expansion",
        ]
    );
}

#[test]
fn preparing_a_change_alters_nothing() {
    let mut client = Client::connect();
    client.write("src/A.java", "original\n");

    let prepared = client.call(
        "prepare_change",
        json!({ "operation": "replace", "path": "src/A.java", "contents": "changed\n" }),
    );

    assert!(!prepared["plan_id"].as_str().expect("plan id").is_empty());
    assert_eq!(prepared["operation"], "replace");
    assert_eq!(prepared["paths"], json!(["src/A.java"]));
    assert_eq!(
        client.read("src/A.java"),
        "original\n",
        "planning touches nothing"
    );
}

#[test]
fn a_prepared_change_can_be_applied() {
    let mut client = Client::connect();

    let prepared = client.call(
        "prepare_change",
        json!({ "operation": "create", "path": "src/New.java", "contents": "new\n" }),
    );
    let applied = client.call("apply_change", json!({ "plan_id": prepared["plan_id"] }));

    assert_eq!(applied["stage"], "committed");
    assert_eq!(applied["operation"], "create");
    assert_eq!(client.read("src/New.java"), "new\n");
}

#[test]
fn every_operation_goes_through() {
    let mut client = Client::connect();
    client.write("src/A.java", "one\n");

    for (arguments, check) in [
        (
            json!({ "operation": "replace", "path": "src/A.java", "contents": "two\n" }),
            "replace",
        ),
        (
            json!({ "operation": "move", "path": "src/A.java", "to": "src/B.java" }),
            "move",
        ),
        (
            json!({ "operation": "trash", "path": "src/B.java" }),
            "trash",
        ),
    ] {
        let prepared = client.call("prepare_change", arguments);
        assert_eq!(prepared["operation"], check);
        let applied = client.call("apply_change", json!({ "plan_id": prepared["plan_id"] }));
        assert_eq!(applied["stage"], "committed");
    }

    assert!(!client.exists("src/A.java"));
    assert!(!client.exists("src/B.java"));
}

#[test]
fn a_refusal_carries_its_code_and_its_hint() {
    // The hint is what stops a client retrying something that can never work, so
    // losing it on the way out would undo the work every denial does to carry one.
    let mut client = Client::connect();

    let error = client.call_expecting_refusal(
        "prepare_change",
        json!({ "operation": "create", "path": "docs/Notes.md", "contents": "x\n" }),
    );

    assert_eq!(error["data"]["code"], "SCOPE_DENIED");
    assert_eq!(error["data"]["retryable"], json!(false));
    assert!(
        error["data"]["hint"]
            .as_str()
            .expect("hint")
            .contains("expansion"),
        "{error}"
    );
    assert!(!client.exists("docs/Notes.md"));
}

#[test]
fn a_denied_path_never_reaches_the_filesystem() {
    let mut client = Client::connect();
    let error = client.call_expecting_refusal(
        "prepare_change",
        json!({ "operation": "create", "path": "src/.env", "contents": "SECRET=1\n" }),
    );

    assert_eq!(error["data"]["code"], "SCOPE_DENIED");
    assert!(!client.exists("src/.env"));
}

#[test]
fn apply_takes_a_plan_id_and_nothing_else() {
    // The point of the split: a client cannot hand back different contents than
    // the ones that were checked, because it has nowhere to put them.
    let mut client = Client::connect();
    let response = client.request("tools/list", json!({}));

    let apply = response["result"]["tools"]
        .as_array()
        .expect("tools")
        .iter()
        .find(|tool| tool["name"] == "apply_change")
        .expect("apply_change")
        .clone();

    let properties = apply["inputSchema"]["properties"]
        .as_object()
        .expect("properties");
    let mut fields: Vec<&String> = properties.keys().collect();
    fields.sort();
    assert_eq!(fields, ["plan_id", "request_id"]);
}

#[test]
fn an_unknown_plan_is_refused() {
    let mut client = Client::connect();
    let error = client.call_expecting_refusal(
        "apply_change",
        json!({ "plan_id": "plan_00000000000000000000000000000001" }),
    );
    assert_eq!(error["data"]["code"], "PLAN_NOT_FOUND");
}

#[test]
fn a_plan_cannot_be_applied_twice() {
    let mut client = Client::connect();
    let prepared = client.call(
        "prepare_change",
        json!({ "operation": "create", "path": "src/Once.java", "contents": "once\n" }),
    );

    client.call("apply_change", json!({ "plan_id": prepared["plan_id"] }));
    let error =
        client.call_expecting_refusal("apply_change", json!({ "plan_id": prepared["plan_id"] }));

    assert_eq!(error["data"]["code"], "PLAN_NOT_FOUND");
    assert_eq!(client.read("src/Once.java"), "once\n");
}

#[test]
fn status_reports_usage_and_says_what_is_not_covered() {
    let mut client = Client::connect();
    let prepared = client.call(
        "prepare_change",
        json!({ "operation": "create", "path": "src/A.java", "contents": "x\n" }),
    );
    client.call("apply_change", json!({ "plan_id": prepared["plan_id"] }));

    let status = client.call("get_status", json!({}));
    assert_eq!(status["changed_paths"]["used"], 1);
    assert_eq!(status["operations"]["used"], 1);
    assert_eq!(status["unsettled"], 0);
    assert_eq!(status["needs_attention"], 0);
    assert_eq!(status["unapproved_policy_edits"], json!(false));
    assert_eq!(status["allowed"][0]["pattern"], "src/**");

    // The gap is stated rather than implied.
    let coverage = status["coverage"].as_str().expect("coverage");
    assert!(coverage.contains("shell"), "{coverage}");
}

#[test]
fn status_reports_an_unapproved_policy_edit() {
    let mut client = Client::connect();
    assert_eq!(
        client.call("get_status", json!({}))["unapproved_policy_edits"],
        json!(false)
    );

    std::fs::write(
        client.workspace().join(".safescope/policy.toml"),
        format!("{POLICY}\n# edited\n"),
    )
    .expect("edit");

    assert_eq!(
        client.call("get_status", json!({}))["unapproved_policy_edits"],
        json!(true)
    );
}

#[test]
fn history_lists_what_was_done_most_recent_first() {
    let mut client = Client::connect();
    for name in ["A", "B"] {
        let prepared = client.call(
            "prepare_change",
            json!({
                "operation": "create",
                "path": format!("src/{name}.java"),
                "contents": "x\n",
            }),
        );
        client.call("apply_change", json!({ "plan_id": prepared["plan_id"] }));
    }

    let history = client.call("get_history", json!({}));
    let entries = history.as_array().expect("entries");
    assert_eq!(entries.len(), 2);
    assert_eq!(
        entries[0]["paths"],
        json!(["src/B.java"]),
        "most recent first"
    );
    assert_eq!(entries[0]["kind"], "change");
    assert_eq!(entries[0]["stage"], "committed");
}

#[test]
fn an_operation_can_be_undone() {
    let mut client = Client::connect();
    client.write("src/A.java", "original\n");

    let prepared = client.call(
        "prepare_change",
        json!({ "operation": "replace", "path": "src/A.java", "contents": "changed\n" }),
    );
    client.call("apply_change", json!({ "plan_id": prepared["plan_id"] }));
    assert_eq!(client.read("src/A.java"), "changed\n");

    let undo = client.call("prepare_undo", json!({}));
    assert_eq!(
        client.read("src/A.java"),
        "changed\n",
        "preparing changes nothing"
    );

    client.call("apply_undo", json!({ "plan_id": undo["plan_id"] }));
    assert_eq!(client.read("src/A.java"), "original\n");
}

#[test]
fn an_undo_is_refused_when_the_file_has_moved_on() {
    let mut client = Client::connect();
    client.write("src/A.java", "original\n");

    let prepared = client.call(
        "prepare_change",
        json!({ "operation": "replace", "path": "src/A.java", "contents": "changed\n" }),
    );
    client.call("apply_change", json!({ "plan_id": prepared["plan_id"] }));
    client.write("src/A.java", "somebody edited it\n");

    let error = client.call_expecting_refusal("prepare_undo", json!({}));
    assert_eq!(error["data"]["code"], "RECOVERY_CONFLICT");
    assert_eq!(client.read("src/A.java"), "somebody edited it\n");
}

#[test]
fn undo_appears_in_history_as_an_undo() {
    let mut client = Client::connect();
    let prepared = client.call(
        "prepare_change",
        json!({ "operation": "create", "path": "src/A.java", "contents": "x\n" }),
    );
    client.call("apply_change", json!({ "plan_id": prepared["plan_id"] }));

    let undo = client.call("prepare_undo", json!({}));
    client.call("apply_undo", json!({ "plan_id": undo["plan_id"] }));

    let history = client.call("get_history", json!({}));
    assert_eq!(history[0]["kind"], "undo");
    assert_eq!(history[1]["kind"], "change");
}

#[test]
fn a_move_needs_a_destination() {
    let mut client = Client::connect();
    client.write("src/A.java", "x\n");

    let error = client.call_expecting_refusal(
        "prepare_change",
        json!({ "operation": "move", "path": "src/A.java" }),
    );
    assert!(
        error["message"].as_str().expect("message").contains("to"),
        "{error}"
    );
}

#[test]
fn a_traversing_path_is_refused() {
    let mut client = Client::connect();
    let error = client.call_expecting_refusal(
        "prepare_change",
        json!({ "operation": "create", "path": "../escape.txt", "contents": "x\n" }),
    );
    assert_eq!(error["data"]["code"], "INVALID_PATH");
}
