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

// Windows makes a symlink only with Developer Mode on or from an elevated
// prompt. The CI runner has that; a machine without it fails here and says why,
// rather than passing without having asked the question — the refusals these
// cover were wrong on Windows for as long as they did not run there.
#[cfg(windows)]
const WINDOWS_SYMLINK: &str = "symlink: Windows needs Developer Mode or an elevated prompt";

/// A symlink at `link` naming the file `target`.
fn symlink_file(target: &Path, link: &Path) {
    #[cfg(unix)]
    std::os::unix::fs::symlink(target, link).expect("symlink");
    #[cfg(windows)]
    std::os::windows::fs::symlink_file(target, link).expect(WINDOWS_SYMLINK);
}

/// A symlink at `link` naming the directory `target`.
fn symlink_dir(target: &Path, link: &Path) {
    #[cfg(unix)]
    std::os::unix::fs::symlink(target, link).expect("symlink");
    #[cfg(windows)]
    std::os::windows::fs::symlink_dir(target, link).expect(WINDOWS_SYMLINK);
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
    symlink_file(
        Path::new("Login.java"),
        &root.path().join("src/auth/Alias.java"),
    );

    let error = workspace.resolve(&path("src/auth/Alias.java")).unwrap_err();
    assert_eq!(error.code(), ErrorCode::UnsupportedOperation);
}

#[test]
fn refuses_a_symlink_in_a_parent_component() {
    // The classic escape: everything about the name looks fine, and the link
    // moves the operation somewhere else entirely.
    let (root, workspace) = workspace();
    fs::create_dir_all(root.path().join("elsewhere")).expect("create dir");
    symlink_dir(
        &Path::new("..").join("elsewhere"),
        &root.path().join("src/link"),
    );

    let error = workspace
        .resolve(&path("src/link/Target.java"))
        .unwrap_err();
    assert_eq!(error.code(), ErrorCode::UnsupportedOperation);
}

#[test]
fn refuses_a_symlink_pointing_outside_the_workspace() {
    let (root, workspace) = workspace();
    let outside = TempDir::new().expect("outside");
    write(outside.path(), "secret.txt", b"not in the workspace");
    symlink_file(
        &outside.path().join("secret.txt"),
        &root.path().join("src/auth/Escape.java"),
    );

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

/// What the refusal of a symlink actually rests on.
///
/// Both the component walk and the target read depend on `cap-std` refusing to
/// follow a link, not on the check that runs beside them — a check and an open
/// are two questions about a name that can change in between. These pin the
/// dependency's behaviour, so a version that started following links fails here
/// rather than in somebody's workspace.
mod what_the_refusal_rests_on {
    use super::*;

    #[test]
    fn opening_the_target_does_not_follow_a_link() {
        let root = TempDir::new().expect("workspace");
        std::fs::write(root.path().join("real.txt"), "the other file").expect("write");
        symlink_file(Path::new("real.txt"), &root.path().join("link.txt"));

        let directory =
            cap_std::fs::Dir::open_ambient_dir(root.path(), cap_std::ambient_authority())
                .expect("open");

        let opened = safescope::path_guard::open_without_following(&directory, "link.txt");
        assert!(
            opened.is_err(),
            "the link was followed, so a target swapped after the check would be read"
        );
    }

    #[test]
    fn opening_the_target_still_works_on_a_real_file() {
        // The refusal has to cost nothing in the ordinary case.
        let root = TempDir::new().expect("workspace");
        std::fs::write(root.path().join("real.txt"), "contents").expect("write");

        let directory =
            cap_std::fs::Dir::open_ambient_dir(root.path(), cap_std::ambient_authority())
                .expect("open");

        assert!(safescope::path_guard::open_without_following(&directory, "real.txt").is_ok());
    }

    #[test]
    fn opening_a_directory_component_does_not_follow_a_link() {
        // The component walk checks and then calls `open_dir`. The check is for
        // the message; this is what makes the walk safe.
        let root = TempDir::new().expect("workspace");
        std::fs::create_dir(root.path().join("real")).expect("mkdir");
        symlink_dir(Path::new("real"), &root.path().join("link"));

        let directory =
            cap_std::fs::Dir::open_ambient_dir(root.path(), cap_std::ambient_authority())
                .expect("open");

        use cap_fs_ext::DirExt as _;
        assert!(
            directory.open_dir_nofollow("link").is_err(),
            "a symlinked directory component was opened, so the walk can be diverted"
        );
        assert!(
            directory.open_dir_nofollow("real").is_ok(),
            "a real one still opens"
        );

        // And the plain one does follow, which is why the walk must not use it.
        assert!(
            directory.open_dir("link").is_ok(),
            "cap-std stopped following; the comment in path_guard should be revisited"
        );
    }
}
