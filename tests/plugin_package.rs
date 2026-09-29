//! The plugin package.
//!
//! A manifest is only worth as much as the command it names. These check that
//! the files exist, that the schemas are the ones the host documents, and that
//! the argv written into the manifests actually works — a plugin whose binary
//! never answers is a plugin that silently does nothing.

use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use serde_json::{Value, json};
use tempfile::TempDir;

const BINARY: &str = env!("CARGO_BIN_EXE_safescope");

fn plugin_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("plugin")
}

fn read_json(relative: &str) -> Value {
    let path = plugin_root().join(relative);
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("{}: {error}", path.display()));
    serde_json::from_str(&text)
        .unwrap_or_else(|error| panic!("{} is not valid JSON: {error}", path.display()))
}

/// The argv a manifest asks for, with the plugin root standing in for the
/// binary the build script installs there.
/// Assembles an entry the way the host does: `command` is the executable and
/// `args` are its arguments.
///
/// Reading `args` alone once hid a manifest that named the binary in `args[0]`
/// and left `command` out. The host requires `command`, so it never ran the
/// hook at all — while this test, which built the argv itself, went on passing.
fn argv(entry: &Value, workspace: &Path) -> Vec<String> {
    let expand = |text: &str| {
        text.replace("${CLAUDE_PLUGIN_ROOT}/bin/safescope.exe", BINARY)
            .replace("${CLAUDE_PROJECT_DIR}", &workspace.to_string_lossy())
    };
    let command = entry["command"]
        .as_str()
        .expect("every command entry needs a `command`: the host will not run one without it");
    std::iter::once(expand(command))
        .chain(
            entry["args"]
                .as_array()
                .into_iter()
                .flatten()
                .map(|argument| expand(argument.as_str().expect("argument"))),
        )
        .collect()
}

/// Runs a JSON-RPC conversation against the server and returns its replies.
fn converse(command: &[String], cwd: &Path, requests: &[Value]) -> Vec<Value> {
    // The manifest's own `command`, not a hardcoded path: naming the executable
    // is part of what is under test.
    let mut child = Command::new(&command[0])
        .args(&command[1..])
        .current_dir(cwd)
        .env("SAFESCOPE_LANG", "en")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("the manifest's command runs");

    let mut script = String::new();
    script.push_str(
        &json!({
            "jsonrpc": "2.0", "id": 1, "method": "initialize",
            "params": {
                "protocolVersion": "2025-06-18",
                "capabilities": {},
                "clientInfo": { "name": "test", "version": "1" },
            },
        })
        .to_string(),
    );
    script.push('\n');
    script
        .push_str(&json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }).to_string());
    script.push('\n');
    for request in requests {
        script.push_str(&request.to_string());
        script.push('\n');
    }

    child
        .stdin
        .take()
        .expect("stdin")
        .write_all(script.as_bytes())
        .expect("write");
    let output = child.wait_with_output().expect("wait");
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| serde_json::from_str(line).ok())
        .collect()
}

/// How many tools the server offers.
fn list_tools(command: &[String], cwd: &Path) -> usize {
    let replies = converse(
        command,
        cwd,
        &[json!({
            "jsonrpc": "2.0", "id": 2, "method": "tools/list",
        })],
    );
    replies
        .iter()
        .find(|reply| reply.get("id") == Some(&json!(2)))
        .and_then(|reply| reply["result"]["tools"].as_array())
        .map_or(0, Vec::len)
}

/// The error a tool call returns.
fn call_tool(command: &[String], cwd: &Path, tool: &str) -> Value {
    let replies = converse(
        command,
        cwd,
        &[json!({
            "jsonrpc": "2.0", "id": 3, "method": "tools/call",
            "params": { "name": tool, "arguments": {} },
        })],
    );
    replies
        .iter()
        .find(|reply| reply.get("id") == Some(&json!(3)))
        .and_then(|reply| reply.get("error"))
        .cloned()
        .unwrap_or_else(|| panic!("{tool} was expected to fail: {replies:?}"))
}

#[test]
fn the_manifest_names_files_that_exist() {
    let manifest = read_json(".claude-plugin/plugin.json");
    assert_eq!(manifest["name"], "safescope");

    for field in ["skills", "hooks", "mcpServers"] {
        let relative = manifest[field].as_str().expect(field);
        let path = plugin_root().join(relative.trim_start_matches("./"));
        assert!(
            path.exists(),
            "{field} points at {}, which does not exist",
            path.display()
        );
    }
}

#[test]
fn every_skill_declares_itself() {
    // A skill without a name falls back to its directory, and one without a
    // description is one a model cannot decide to use.
    let skills = std::fs::read_dir(plugin_root().join("skills")).expect("skills");
    let mut found = 0;

    for entry in skills.flatten() {
        let file = entry.path().join("SKILL.md");
        assert!(file.is_file(), "{} has no SKILL.md", entry.path().display());

        let text = std::fs::read_to_string(&file).expect("read");
        let front = text
            .strip_prefix("---\n")
            .and_then(|rest| rest.split_once("\n---\n"))
            .expect("YAML frontmatter")
            .0;
        assert!(front.contains("name:"), "{}: no name", file.display());
        assert!(
            front.contains("description:"),
            "{}: no description",
            file.display()
        );
        found += 1;
    }

    assert_eq!(
        found, 6,
        "the manifest and the README both describe six skills"
    );
}

#[test]
fn the_hook_manifest_covers_the_edit_tools_and_the_session() {
    let hooks = read_json("hooks/hooks.json");
    let events = hooks["hooks"].as_object().expect("hooks");

    // Every tool, so the hook decides with the policy in front of it rather than
    // the manifest deciding for it. Naming the edit tools here meant the hook
    // never saw the routes an agent actually took when they were denied.
    //
    // Seeing Bash is still not judging Bash: the hook says nothing about a shell
    // command in the default mode, and refuses one under an allowlist by not
    // finding it on the list, never by reading it.
    let matcher = events["PreToolUse"][0]["matcher"]
        .as_str()
        .expect("matcher");
    assert_eq!(matcher, "*", "the hook should be offered every tool call");

    assert!(events.contains_key("SessionStart"));
    assert!(events.contains_key("Stop"));

    // Every entry names its executable in `command`. A manifest that puts it in
    // `args[0]` and omits `command` parses, validates and does nothing: the host
    // skips the entry silently, so the hook that is supposed to be the first
    // line of the report never runs.
    for (event, groups) in events {
        for group in groups.as_array().expect("groups") {
            for entry in group["hooks"].as_array().expect("hooks") {
                assert!(
                    entry["command"].is_string(),
                    "{event}: a command hook without `command` is never run"
                );
            }
        }
    }
}

#[test]
fn the_hook_command_the_manifest_names_actually_answers() {
    let workspace = TempDir::new().expect("workspace");
    let hooks = read_json("hooks/hooks.json");
    let command = argv(
        &hooks["hooks"]["PreToolUse"][0]["hooks"][0],
        workspace.path(),
    );

    let mut child = Command::new(&command[0])
        .args(&command[1..])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("the manifest's command runs");

    let payload = json!({
        "hook_event_name": "PreToolUse",
        "cwd": workspace.path().to_string_lossy(),
        "tool_name": "Write",
        "tool_input": { "file_path": workspace.path().join("a.rs").to_string_lossy() },
    });
    child
        .stdin
        .take()
        .expect("stdin")
        .write_all(payload.to_string().as_bytes())
        .expect("write");

    let output = child.wait_with_output().expect("wait");
    assert_eq!(
        output.status.code(),
        Some(0),
        "a hook must not exit non-zero: anything but 0 or 2 is a non-blocking error"
    );
}

#[test]
fn the_mcp_command_the_manifest_names_actually_serves() {
    // On a workspace SafeScope knows nothing about. An earlier version refused
    // to start here, and this test asserted that as if it were desirable — but a
    // server that never appears is one Claude cannot tell apart from a plugin
    // that was never installed, so it silently falls back to whatever else it
    // has. Starting and saying what is wrong is the more useful failure.
    let workspace = TempDir::new().expect("workspace");
    let servers = read_json(".mcp.json");
    let server = &servers["mcpServers"]["safescope"];

    assert!(
        server["command"]
            .as_str()
            .expect("command")
            .contains("${CLAUDE_PLUGIN_ROOT}"),
        "the binary is resolved relative to the plugin, not found on PATH"
    );
    // ${CLAUDE_PROJECT_DIR} is not expanded in these arguments — the server
    // received it literally and died — so the workspace comes from the working
    // directory the host launches it in.
    for argument in server["args"].as_array().expect("args") {
        assert!(
            !argument
                .as_str()
                .expect("argument")
                .contains("${CLAUDE_PROJECT_DIR}"),
            "this placeholder is not expanded here: {argument}"
        );
    }

    let tools = list_tools(&argv(server, workspace.path()), workspace.path());
    assert_eq!(
        tools, 7,
        "the server offers its tools even on a bare directory"
    );
}

#[test]
fn a_tool_call_on_an_unknown_workspace_says_what_to_do() {
    let workspace = TempDir::new().expect("workspace");
    let servers = read_json(".mcp.json");
    let command = argv(&servers["mcpServers"]["safescope"], workspace.path());

    let error = call_tool(&command, workspace.path(), "get_status");
    assert_eq!(error["data"]["code"], "WORKSPACE_NOT_REGISTERED");
    assert!(
        error["message"]
            .as_str()
            .expect("message")
            .contains("safescope init"),
        "{error}"
    );
}

#[test]
fn the_readme_does_not_promise_a_sandbox() {
    // The one claim this project must never make.
    let readme = std::fs::read_to_string(plugin_root().join("README.md")).expect("README");
    assert!(
        readme.contains("not a sandbox"),
        "the limitation has to be stated"
    );
    assert!(
        readme.contains("Bash"),
        "the gap Bash leaves has to be named"
    );

    // Real agent sessions wrote files through Bash when Write and Edit were
    // denied, and through Monitor when Bash was denied too. An earlier version
    // of this document called the second configuration "a complete record",
    // which was not true. It must not say so again.
    assert!(
        readme.contains("no configuration that guarantees a complete record"),
        "the README must not promise a configuration that closes the gap"
    );
    assert!(
        readme.contains("Monitor"),
        "the tool that got past a Bash deny has to be named"
    );
}

/// Every executable the manifests name, with the plugin root put where the host
/// puts it.
fn manifest_commands() -> Vec<PathBuf> {
    let root = plugin_root().to_string_lossy().into_owned();
    let hooks = read_json("hooks/hooks.json");
    let servers = read_json(".mcp.json");

    let hook_commands = hooks["hooks"]
        .as_object()
        .expect("hooks by event")
        .values()
        .flat_map(|groups| groups.as_array().expect("matcher groups").iter())
        .flat_map(|group| group["hooks"].as_array().expect("handlers").iter())
        .map(|handler| handler["command"].as_str().expect("command"));
    let server_commands = servers["mcpServers"]
        .as_object()
        .expect("servers")
        .values()
        .map(|server| server["command"].as_str().expect("command"));

    hook_commands
        .chain(server_commands)
        .map(|command| PathBuf::from(command.replace("${CLAUDE_PLUGIN_ROOT}", &root)))
        .collect()
}

#[test]
fn every_manifest_names_the_binary_by_a_name_windows_will_run() {
    // On Windows the host requires a hook's command to "resolve to a real
    // executable such as a .exe", and documents nothing about finding one from
    // a name without the suffix. An extensionless name might work; `.exe` is
    // what the documentation promises, so it is what every manifest says.
    let commands = manifest_commands();
    assert!(!commands.is_empty());
    for command in &commands {
        assert_eq!(
            command.extension().and_then(|extension| extension.to_str()),
            Some("exe"),
            "{} is not a name the host documents running on Windows",
            command.display()
        );
    }
}

#[test]
fn the_packaged_plugin_answers_where_the_manifest_says_it_is() {
    // Every other test here substitutes the test binary for the path the
    // manifest names, which proves the engine answers and says nothing about
    // whether the plugin somebody installs has one. The binary is built by a
    // script and not checked in, so a clone that skipped it has a manifest
    // pointing at nothing — and a plugin whose binary never answers is a plugin
    // that silently does nothing, which is the failure this project exists to
    // stop shipping.
    //
    // The path comes from the manifests, not from this test. An earlier version
    // named the file itself, and so could not have noticed a manifest and a
    // build script that disagreed about what it was called.
    let commands = manifest_commands();
    let packaged = commands.first().expect("a command").clone();
    assert!(
        commands.iter().all(|command| *command == packaged),
        "the manifests name different binaries: {commands:?}"
    );
    assert!(
        packaged.is_file(),
        "{} is missing: run ./scripts/build-plugin.sh",
        packaged.display()
    );

    let workspace = TempDir::new().expect("workspace");
    let output = Command::new(&packaged)
        .arg("--workspace")
        .arg(workspace.path())
        .arg("--help")
        .env("SAFESCOPE_LANG", "en")
        .output()
        .expect("the packaged binary runs");

    assert!(
        output.status.success(),
        "the packaged binary did not answer: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let help = String::from_utf8_lossy(&output.stdout);
    for command in ["guard", "drift", "mcp", "hook"] {
        assert!(
            help.contains(command),
            "the packaged binary is missing `{command}`; it may be a stale build"
        );
    }
}
