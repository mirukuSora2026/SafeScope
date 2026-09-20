//! The MCP server, driven the way a client drives it.
//!
//! Real JSON-RPC over the real binary's stdio. Calling the tool functions
//! directly would test the author's idea of the protocol rather than the
//! protocol, and the wire shape is the part a client actually depends on.

use std::io::{BufRead as _, BufReader, Write as _};
use std::path::Path;
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

use safescope::cli::approve;
use safescope::registry;
use safescope::store::DATA_DIR_ENV;
use serde_json::{Value, json};
use tempfile::TempDir;

const BINARY: &str = env!("CARGO_BIN_EXE_safescope");

const POLICY: &str = "\
schema_version = 1

[scope]
allow = [\"src/**\"]
deny = [\"**/.env\"]
";

fn env_lock() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// A running server with a request/response channel to it.
struct Client {
    _data: TempDir,
    root: TempDir,
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    next_id: i64,
}

impl Client {
    /// Registers a workspace, approves `POLICY`, and completes the handshake.
    fn connect() -> Self {
        let data = TempDir::new().expect("data");
        let root = TempDir::new().expect("workspace");

        {
            let _guard = env_lock();
            unsafe { std::env::set_var(DATA_DIR_ENV, data.path()) };
            std::fs::create_dir_all(root.path().join("src")).expect("mkdir");
            let registration = registry::init(root.path()).expect("init");
            std::fs::write(registration.policy_path(), POLICY).expect("policy");
            let policy = approve::check_policy(POLICY).expect("valid");
            approve::perform(&registration, policy, POLICY).expect("approve");
        }

        let mut child = Command::new(BINARY)
            .env(DATA_DIR_ENV, data.path())
            .env("SAFESCOPE_LANG", "en")
            .arg("--workspace")
            .arg(root.path())
            .arg("mcp")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn server");

        let mut client = Self {
            stdin: child.stdin.take().expect("stdin"),
            stdout: BufReader::new(child.stdout.take().expect("stdout")),
            child,
            _data: data,
            root,
            next_id: 0,
        };

        // Declaring elicitation is what tells the engine this client could put a
        // question to a person.
        client.request(
            "initialize",
            json!({
                "protocolVersion": "2025-06-18",
                "capabilities": { "elicitation": {} },
                "clientInfo": { "name": "claude-code", "version": "1.0" },
            }),
        );
        client.notify("notifications/initialized");
        client
    }

    fn request(&mut self, method: &str, params: Value) -> Value {
        self.next_id += 1;
        let id = self.next_id;
        let message = json!({
            "jsonrpc": "2.0", "id": id, "method": method, "params": params,
        });
        writeln!(self.stdin, "{message}").expect("write");
        self.stdin.flush().expect("flush");

        loop {
            let mut line = String::new();
            let read = self.stdout.read_line(&mut line).expect("read");
            assert!(read > 0, "the server closed while answering {method}");
            let Ok(response) = serde_json::from_str::<Value>(&line) else {
                continue;
            };
            if response.get("id") == Some(&json!(id)) {
                return response;
            }
        }
    }

    fn notify(&mut self, method: &str) {
        let message = json!({ "jsonrpc": "2.0", "method": method });
        writeln!(self.stdin, "{message}").expect("write");
        self.stdin.flush().expect("flush");
    }

    /// Calls a tool and returns its structured result.
    fn call(&mut self, tool: &str, arguments: Value) -> Value {
        let response = self.request(
            "tools/call",
            json!({ "name": tool, "arguments": arguments }),
        );
        assert!(
            response.get("error").is_none(),
            "{tool} failed: {}",
            response["error"]
        );
        let result = &response["result"];
        assert_ne!(
            result["isError"],
            json!(true),
            "{tool} reported an error: {result}"
        );
        result["structuredContent"].clone()
    }

    /// Calls a tool expecting it to be refused, and returns the error.
    fn call_expecting_refusal(&mut self, tool: &str, arguments: Value) -> Value {
        let response = self.request(
            "tools/call",
            json!({ "name": tool, "arguments": arguments }),
        );
        response
            .get("error")
            .cloned()
            .unwrap_or_else(|| panic!("{tool} was expected to be refused: {response}"))
    }

    fn write(&self, relative: &str, contents: &str) {
        std::fs::write(self.root.path().join(relative), contents).expect("write");
    }

    fn read(&self, relative: &str) -> String {
        std::fs::read_to_string(self.root.path().join(relative)).expect("read")
    }

    fn exists(&self, relative: &str) -> bool {
        self.root.path().join(relative).exists()
    }

    fn workspace(&self) -> &Path {
        self.root.path()
    }
}

impl Drop for Client {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[test]
fn the_server_offers_six_tools() {
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
