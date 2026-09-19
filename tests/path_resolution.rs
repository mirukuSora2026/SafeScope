//! Resolving validated paths against a real filesystem.
//!
//! These run against temporary directories rather than a mock, because the
//! behaviour under test is exactly what a filesystem does and a mock would only
//! confirm the author's assumptions about it.

use std::fs;
use std::io::Write as _;
use std::path::Path;

use safescope::domain::FileState;
use safescope::error::ErrorCode;
use safescope::hash::ContentHash;
use safescope::path_guard::Workspace;
use safescope::paths::RelPath;
use tempfile::TempDir;

fn path(text: &str) -> RelPath {
    RelPath::parse(text).unwrap()
}

/// A workspace with `src/auth/Login.java` in it.
fn workspace() -> (TempDir, Workspace) {
    let root = TempDir::new().expect("temp dir");
    fs::create_dir_all(root.path().join("src/auth")).expect("create dirs");
    write(root.path(), "src/auth/Login.java", b"class Login {}\n");
    let handle = Workspace::open(root.path()).expect("open workspace");
    (root, handle)
}

fn write(root: &Path, relative: &str, contents: &[u8]) {
    let mut file = fs::File::create(root.join(relative)).expect("create file");
    file.write_all(contents).expect("write file");
}

#[test]
fn reports_an_existing_file_with_its_hash_and_length() {
    let (_root, workspace) = workspace();
    let resolved = workspace
        .resolve(&path("src/auth/Login.java"))
        .expect("resolves");

    assert!(resolved.exists());
    assert_eq!(resolved.file_name(), "Login.java");
    assert_eq!(
        resolved.state(),
        &FileState::present(ContentHash::of_bytes(b"class Login {}\n"), 15)
    );
}

#[test]
fn reports_an_absent_file_without_failing() {
    // A create operation needs to know the target is absent, which is not an
    // error condition.
    let (_root, workspace) = workspace();
    let resolved = workspace
        .resolve(&path("src/auth/New.java"))
        .expect("resolves");
    assert!(!resolved.exists());
    assert_eq!(resolved.state(), &FileState::Absent);
}

#[test]
fn the_parent_handle_reaches_the_target() {
    // The executor acts through this handle instead of re-walking the path, so
    // it has to be the directory the file actually lives in.
    let (_root, workspace) = workspace();
    let resolved = workspace
        .resolve(&path("src/auth/Login.java"))
        .expect("resolves");
    let metadata = resolved
        .parent()
        .metadata(resolved.file_name())
        .expect("the handle can see the file");
    assert!(metadata.is_file());
}

#[test]
fn refuses_a_symlink_as_the_target() {
    let (root, workspace) = workspace();
    std::os::unix::fs::symlink("Login.java", root.path().join("src/auth/Alias.java"))
        .expect("symlink");

    let error = workspace.resolve(&path("src/auth/Alias.java")).unwrap_err();
    assert_eq!(error.code(), ErrorCode::UnsupportedOperation);
}

#[test]
fn refuses_a_symlink_in_a_parent_component() {
    // The classic escape: everything about the name looks fine, and the link
    // moves the operation somewhere else entirely.
    let (root, workspace) = workspace();
    fs::create_dir_all(root.path().join("elsewhere")).expect("create dir");
    std::os::unix::fs::symlink("../elsewhere", root.path().join("src/link")).expect("symlink");

    let error = workspace
        .resolve(&path("src/link/Target.java"))
        .unwrap_err();
    assert_eq!(error.code(), ErrorCode::UnsupportedOperation);
}

#[test]
fn refuses_a_symlink_pointing_outside_the_workspace() {
    let (root, workspace) = workspace();
    std::os::unix::fs::symlink("/etc/hosts", root.path().join("src/auth/Escape.java"))
        .expect("symlink");

    let error = workspace
        .resolve(&path("src/auth/Escape.java"))
        .unwrap_err();
    assert_eq!(error.code(), ErrorCode::UnsupportedOperation);
}

#[test]
fn refuses_a_directory_as_the_target() {
    let (_root, workspace) = workspace();
    let error = workspace.resolve(&path("src/auth")).unwrap_err();
    assert_eq!(error.code(), ErrorCode::UnsupportedOperation);
}

#[test]
fn refuses_a_component_that_is_a_file_rather_than_a_directory() {
    let (_root, workspace) = workspace();
    let error = workspace
        .resolve(&path("src/auth/Login.java/Nested.java"))
        .unwrap_err();
    assert_eq!(error.code(), ErrorCode::UnsupportedOperation);
}

#[test]
fn reports_a_missing_parent_directory() {
    // v1 never creates directories: a new directory has no snapshot, no budget
    // line and no defined undo.
    let (_root, workspace) = workspace();
    let error = workspace
        .resolve(&path("src/auth/token/Jwt.java"))
        .unwrap_err();
    assert_eq!(error.code(), ErrorCode::ParentMissing);
    assert!(
        error.is_denial(),
        "a missing directory is a refusal, not a fault"
    );

    let report = error.report();
    assert!(
        report.hint.is_some(),
        "the caller is told what to do instead"
    );
    assert!(!report.retryable);
}

#[test]
fn resolves_a_top_level_file() {
    let (root, workspace) = workspace();
    write(root.path(), "README.md", b"# hi\n");
    let resolved = workspace.resolve(&path("README.md")).expect("resolves");
    assert!(resolved.exists());
    assert_eq!(resolved.file_name(), "README.md");
}

#[test]
fn hashes_a_file_larger_than_the_read_buffer() {
    let (root, workspace) = workspace();
    let contents = vec![b'x'; 256 * 1024 + 7];
    write(root.path(), "src/auth/Big.java", &contents);

    let resolved = workspace
        .resolve(&path("src/auth/Big.java"))
        .expect("resolves");
    assert_eq!(
        resolved.state(),
        &FileState::present(ContentHash::of_bytes(&contents), contents.len() as u64)
    );
}

#[test]
fn an_empty_file_is_present_not_absent() {
    let (root, workspace) = workspace();
    write(root.path(), "src/auth/Empty.java", b"");

    let resolved = workspace
        .resolve(&path("src/auth/Empty.java"))
        .expect("resolves");
    assert!(resolved.exists(), "an empty file still exists");
    assert_eq!(
        resolved.state(),
        &FileState::present(ContentHash::of_bytes(b""), 0)
    );
}

#[test]
fn opening_a_missing_workspace_is_a_fault_not_a_refusal() {
    let root = TempDir::new().expect("temp dir");
    let missing = root.path().join("nope");
    let error = Workspace::open(&missing).unwrap_err();
    assert_eq!(error.code(), ErrorCode::IoFailed);
    assert!(!error.is_denial());
}
