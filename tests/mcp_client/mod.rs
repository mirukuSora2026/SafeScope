//! A JSON-RPC client for driving the real server binary over stdio.
//!
//! Shared by the MCP test files. It answers server-to-client requests as a real
//! client would, which is the only way to exercise an approval that a person is
//! supposed to grant.

#![allow(dead_code)]

use std::io::{BufRead as _, BufReader, Write as _};
use std::path::Path;
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

use safescope::cli::approve;
use safescope::registry;
use safescope::store::{DATA_DIR_ENV, StatePaths};
use serde_json::{Value, json};
use tempfile::TempDir;

const BINARY: &str = env!("CARGO_BIN_EXE_safescope");

pub const POLICY: &str = "\
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
pub struct Client {
    _data: TempDir,
    pub root: TempDir,
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    next_id: i64,
    /// How to answer the next elicitation the server sends.
    ///
    /// A real client puts the question to a person; a test decides in advance
    /// what that person would say.
    answer: Option<Answer>,
}

/// What a person would say to an elicitation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Answer {
    Accept,
    Decline,
    Cancel,
}

impl Answer {
    fn as_str(self) -> &'static str {
        match self {
            Answer::Accept => "accept",
            Answer::Decline => "decline",
            Answer::Cancel => "cancel",
        }
    }
}

impl Client {
    /// Registers a workspace, approves `POLICY`, and completes the handshake.
    pub fn connect() -> Self {
        let mut client = Self::spawn();
        client.handshake(true);
        client
    }

    /// Starts a server and a channel to it, without handshaking.
    fn spawn() -> Self {
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

        Self {
            stdin: child.stdin.take().expect("stdin"),
            stdout: BufReader::new(child.stdout.take().expect("stdout")),
            child,
            _data: data,
            root,
            next_id: 0,
            answer: None,
        }
    }

    /// Connects as a client that cannot put a question to a person.
    pub fn connect_without_elicitation() -> Self {
        let mut client = Self::spawn();
        client.handshake(false);
        client
    }

    /// Declares this client's capabilities and completes initialization.
    ///
    /// Whether elicitation is declared is what decides, later, whether the
    /// engine may accept an approval through this connection at all.
    fn handshake(&mut self, elicitation: bool) {
        let capabilities = if elicitation {
            json!({ "elicitation": {} })
        } else {
            json!({})
        };
        self.request(
            "initialize",
            json!({
                "protocolVersion": "2025-06-18",
                "capabilities": capabilities,
                "clientInfo": { "name": "claude-code", "version": "1.0" },
            }),
        );
        self.notify("notifications/initialized");
    }

    pub fn request(&mut self, method: &str, params: Value) -> Value {
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
            let Ok(message) = serde_json::from_str::<Value>(&line) else {
                continue;
            };

            // The server can ask the client a question while answering a tool
            // call. A real client would put it to a person; here the test has
            // already decided what that person says.
            if message.get("method") == Some(&json!("elicitation/create")) {
                self.answer_elicitation(&message);
                continue;
            }

            if message.get("id") == Some(&json!(id)) {
                return message;
            }
        }
    }

    /// Replies to a server-to-client elicitation request.
    fn answer_elicitation(&mut self, request: &Value) {
        let answer = self
            .answer
            .expect("the server asked a question this test did not expect");
        let reply = json!({
            "jsonrpc": "2.0",
            "id": request["id"],
            "result": { "action": answer.as_str() },
        });
        writeln!(self.stdin, "{reply}").expect("write");
        self.stdin.flush().expect("flush");
    }

    /// Decides what the person will say to the next question.
    pub fn will_answer(&mut self, answer: Answer) {
        self.answer = Some(answer);
    }

    pub fn notify(&mut self, method: &str) {
        let message = json!({ "jsonrpc": "2.0", "method": method });
        writeln!(self.stdin, "{message}").expect("write");
        self.stdin.flush().expect("flush");
    }

    /// Calls a tool and returns its structured result.
    pub fn call(&mut self, tool: &str, arguments: Value) -> Value {
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
    pub fn call_expecting_refusal(&mut self, tool: &str, arguments: Value) -> Value {
        let response = self.request(
            "tools/call",
            json!({ "name": tool, "arguments": arguments }),
        );
        response
            .get("error")
            .cloned()
            .unwrap_or_else(|| panic!("{tool} was expected to be refused: {response}"))
    }

    pub fn write(&self, relative: &str, contents: &str) {
        std::fs::write(self.root.path().join(relative), contents).expect("write");
    }

    pub fn read(&self, relative: &str) -> String {
        std::fs::read_to_string(self.root.path().join(relative)).expect("read")
    }

    pub fn exists(&self, relative: &str) -> bool {
        self.root.path().join(relative).exists()
    }

    /// Runs `body` with this workspace's state directory in the environment.
    ///
    /// Lets a test act as a second process would — issuing a grant the way
    /// `safescope approve` does, while the server is running and holding the
    /// workspace lock.
    pub fn with_state<T>(&self, body: impl FnOnce(&StatePaths) -> T) -> T {
        let _guard = env_lock();
        unsafe { std::env::set_var(DATA_DIR_ENV, self._data.path()) };
        let registration = registry::load(self.root.path()).expect("registered");
        let paths = registration.state_paths().expect("state paths");
        body(&paths)
    }

    pub fn workspace(&self) -> &Path {
        self.root.path()
    }
}

impl Drop for Client {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
