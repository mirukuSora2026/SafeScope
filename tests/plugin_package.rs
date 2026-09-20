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
fn argv(entry: &Value, workspace: &Path) -> Vec<String> {
    entry["args"]
        .as_array()
        .expect("args")
        .iter()
        .map(|argument| {
            argument
                .as_str()
                .expect("argument")
                .replace("${CLAUDE_PLUGIN_ROOT}/bin/safescope", BINARY)
                .replace("${CLAUDE_PROJECT_DIR}", &workspace.to_string_lossy())
        })
        .collect()
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

    // Bash is deliberately absent: a shell command cannot be read reliably
    // enough to judge, and a check that looked like protection without being it
    // would be worse than none.
    let matcher = events["PreToolUse"][0]["matcher"]
        .as_str()
        .expect("matcher");
    for tool in ["Write", "Edit", "MultiEdit", "NotebookEdit"] {
        assert!(matcher.contains(tool), "{matcher} does not cover {tool}");
    }
    assert!(
        !matcher.contains("Bash"),
        "{matcher} should not claim to judge Bash"
    );

    assert!(events.contains_key("SessionStart"));
    assert!(events.contains_key("Stop"));
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
    let workspace = TempDir::new().expect("workspace");
    let servers = read_json(".mcp.json");
    let server = &servers["mcpServers"]["safescope"];
    let command = argv(server, workspace.path());

    assert!(
        server["command"]
            .as_str()
            .expect("command")
            .contains("${CLAUDE_PLUGIN_ROOT}"),
        "the binary is resolved relative to the plugin, not found on PATH"
    );

    let mut child = Command::new(BINARY)
        .args(&command)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("the manifest's command runs");

    // An unregistered workspace: the server should refuse to start rather than
    // serve a project it knows nothing about.
    let outcome = child.wait().expect("wait");
    assert!(
        !outcome.success(),
        "serving an unregistered project should fail"
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
}
