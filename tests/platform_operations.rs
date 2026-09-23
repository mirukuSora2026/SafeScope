//! The file operations the executor is built from.
//!
//! These run against a real filesystem because the properties under test are
//! properties of the filesystem: whether a no-overwrite rename is actually
//! atomic, and what the kernel returns when it cannot be.

use std::fs;
use std::io::Read as _;

use cap_std::ambient_authority;
use cap_std::fs::Dir;
use safescope::error::ErrorCode;
use safescope::paths::TMP_PREFIX;
use safescope::platform;
use tempfile::TempDir;

fn workspace() -> (TempDir, Dir) {
    let root = TempDir::new().expect("temp dir");
    let handle = Dir::open_ambient_dir(root.path(), ambient_authority()).expect("open");
    (root, handle)
}

fn read(root: &TempDir, name: &str) -> String {
    fs::read_to_string(root.path().join(name)).expect("read")
}

fn entries(root: &TempDir) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(root.path())
        .expect("read dir")
        .filter_map(Result::ok)
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

#[test]
fn a_staged_file_becomes_the_target() {
    let (root, directory) = workspace();

    let staged = platform::stage(&directory, b"hello\n").expect("stage");
    assert!(staged.name().starts_with(TMP_PREFIX));
    staged.promote_new("greeting.txt").expect("promote");

    assert_eq!(read(&root, "greeting.txt"), "hello\n");
    assert_eq!(entries(&root), ["greeting.txt"], "no temporary left behind");
}

#[test]
fn promote_new_refuses_an_existing_destination() {
    // The property the whole design rests on: the kernel, not a prior check,
    // decides whether the destination was free.
    let (root, directory) = workspace();
    fs::write(root.path().join("taken.txt"), "original").expect("write");

    let staged = platform::stage(&directory, b"replacement").expect("stage");
    let error = staged.promote_new("taken.txt").unwrap_err();

    assert_eq!(error.code(), ErrorCode::DestinationExists);
    assert!(error.is_denial(), "a refusal, not a fault");
    assert_eq!(
        read(&root, "taken.txt"),
        "original",
        "the original is untouched"
    );
    assert_eq!(entries(&root), ["taken.txt"], "the temporary is cleaned up");
}

#[test]
fn promote_over_replaces_the_destination() {
    let (root, directory) = workspace();
    fs::write(root.path().join("notes.txt"), "old").expect("write");

    platform::stage(&directory, b"new")
        .expect("stage")
        .promote_over("notes.txt")
        .expect("promote");

    assert_eq!(read(&root, "notes.txt"), "new");
    assert_eq!(entries(&root), ["notes.txt"]);
}

#[test]
fn a_staged_file_that_is_never_promoted_removes_itself() {
    // Anything that fails between writing and renaming leaves no debris, so the
    // only temporaries recovery has to clean up are those left by a crash.
    let (root, directory) = workspace();

    {
        let staged = platform::stage(&directory, b"abandoned").expect("stage");
        assert_eq!(entries(&root).len(), 1);
        drop(staged);
    }

    assert!(
        entries(&root).is_empty(),
        "left behind: {:?}",
        entries(&root)
    );
}

#[test]
fn a_replacement_is_never_visible_half_written() {
    // The reader either sees the old contents or the new ones, because the file
    // reaches its name through a rename rather than a truncate-and-write.
    let (root, directory) = workspace();
    fs::write(root.path().join("page.txt"), "aaaa").expect("write");

    let mut reader = fs::File::open(root.path().join("page.txt")).expect("open");
    platform::stage(&directory, b"bbbbbbbb")
        .expect("stage")
        .promote_over("page.txt")
        .expect("promote");

    // The already-open handle still sees the file it opened, untruncated.
    let mut seen = String::new();
    reader.read_to_string(&mut seen).expect("read");
    assert_eq!(seen, "aaaa");
    assert_eq!(read(&root, "page.txt"), "bbbbbbbb");
}

#[test]
fn a_file_moves_between_directories() {
    let (root, directory) = workspace();
    fs::create_dir_all(root.path().join("from")).expect("mkdir");
    fs::create_dir_all(root.path().join("to")).expect("mkdir");
    fs::write(root.path().join("from/a.txt"), "content").expect("write");

    let from = directory.open_dir("from").expect("open from");
    let to = directory.open_dir("to").expect("open to");
    platform::move_file(&from, "a.txt", &to, "b.txt").expect("move");

    assert!(!root.path().join("from/a.txt").exists());
    assert_eq!(read(&root, "to/b.txt"), "content");
}

#[test]
fn a_move_refuses_an_occupied_destination() {
    let (root, directory) = workspace();
    fs::create_dir_all(root.path().join("from")).expect("mkdir");
    fs::create_dir_all(root.path().join("to")).expect("mkdir");
    fs::write(root.path().join("from/a.txt"), "source").expect("write");
    fs::write(root.path().join("to/a.txt"), "destination").expect("write");

    let from = directory.open_dir("from").expect("open from");
    let to = directory.open_dir("to").expect("open to");
    let error = platform::move_file(&from, "a.txt", &to, "a.txt").unwrap_err();

    assert_eq!(error.code(), ErrorCode::DestinationExists);
    assert_eq!(read(&root, "from/a.txt"), "source", "both files survive");
    assert_eq!(read(&root, "to/a.txt"), "destination");
}

#[test]
fn a_rename_within_one_directory_works() {
    let (root, directory) = workspace();
    fs::write(root.path().join("old.txt"), "content").expect("write");

    platform::move_file(&directory, "old.txt", &directory, "new.txt").expect("move");

    assert!(!root.path().join("old.txt").exists());
    assert_eq!(read(&root, "new.txt"), "content");
}

#[test]
fn removing_a_file_leaves_the_directory_consistent() {
    let (root, directory) = workspace();
    fs::write(root.path().join("gone.txt"), "content").expect("write");

    platform::remove(&directory, "gone.txt").expect("remove");
    assert!(entries(&root).is_empty());
}

#[test]
fn removing_a_missing_file_is_a_fault() {
    let (_root, directory) = workspace();
    let error = platform::remove(&directory, "never-existed.txt").unwrap_err();
    assert_eq!(error.code(), ErrorCode::IoFailed);
}

#[test]
fn an_empty_file_stages_and_promotes() {
    let (root, directory) = workspace();
    platform::stage(&directory, b"")
        .expect("stage")
        .promote_new("empty.txt")
        .expect("promote");
    assert_eq!(read(&root, "empty.txt"), "");
}

#[test]
fn a_large_file_stages_and_promotes() {
    let (root, directory) = workspace();
    let contents = vec![b'x'; 4 * 1024 * 1024];

    platform::stage(&directory, &contents)
        .expect("stage")
        .promote_new("big.bin")
        .expect("promote");

    assert_eq!(
        fs::metadata(root.path().join("big.bin"))
            .expect("metadata")
            .len(),
        contents.len() as u64
    );
}

/// Moving between filesystems.
///
/// Needs a second filesystem, so it runs only when `SAFESCOPE_OTHER_FS` points at
/// one. On macOS:
///
/// ```text
/// DEV=$(hdiutil attach -nomount ram://8192)
/// diskutil eraseVolume HFS+ SFSRAM "$DEV"
/// SAFESCOPE_OTHER_FS=/Volumes/SFSRAM cargo test --test platform_operations
/// ```
#[test]
fn a_move_across_filesystems_is_refused() {
    let Ok(other_root) = std::env::var("SAFESCOPE_OTHER_FS") else {
        eprintln!("skipped: set SAFESCOPE_OTHER_FS to a path on another filesystem");
        return;
    };

    let (root, directory) = workspace();
    fs::write(root.path().join("a.txt"), "content").expect("write");

    let other = Dir::open_ambient_dir(&other_root, ambient_authority())
        .expect("the second filesystem is reachable");
    let error = platform::move_file(&directory, "a.txt", &other, "a.txt").unwrap_err();

    // Refused rather than emulated as copy-then-delete: a crash in the middle of
    // that would leave the file in two places or none, and the journal would
    // have recorded a move.
    assert_eq!(error.code(), ErrorCode::UnsupportedOperation);
    assert!(error.is_denial());
    assert_eq!(read(&root, "a.txt"), "content", "the source is untouched");
    assert!(
        !std::path::Path::new(&other_root).join("a.txt").exists(),
        "nothing was created on the other filesystem"
    );
}

/// A rename failure the way a platform hands one over.
///
/// Two shapes reach [`map_rename_error`] and only one of them carries an errno.
/// Windows classifies before returning, so the error is a wrapper with a kind
/// and no number; Unix returns the raw errno with the kind derived from it.
mod rename_failures {
    use safescope::error::ErrorCode;
    use safescope::platform::map_rename_error;
    use std::io::{Error, ErrorKind};

    #[test]
    fn a_taken_destination_is_a_refusal_even_without_an_errno() {
        // The shape Windows produces. Matching the errno first sent this down
        // the fault path, where a refusal was reported as an engine failure and
        // a caller had no way to tell that retrying could not help.
        let error = Error::new(ErrorKind::AlreadyExists, "destination is taken");
        assert!(error.raw_os_error().is_none(), "the premise of this test");

        let mapped = map_rename_error(&error, "a.rs", "b.rs");
        assert_eq!(mapped.code(), ErrorCode::DestinationExists);
        assert!(
            matches!(mapped, safescope::error::Error::Denied(_)),
            "a rule refusing is not the engine failing"
        );
    }

    #[test]
    fn a_cross_device_move_is_a_refusal_even_without_an_errno() {
        let error = Error::new(ErrorKind::CrossesDevices, "different volume");
        let mapped = map_rename_error(&error, "a.rs", "/other/a.rs");
        assert_eq!(mapped.code(), ErrorCode::UnsupportedOperation);
        assert!(matches!(mapped, safescope::error::Error::Denied(_)));
    }

    #[test]
    fn a_taken_destination_is_still_a_refusal_when_it_is_an_errno() {
        // The shape Unix produces, so the new path does not cost the old one.
        let error = Error::from_raw_os_error(libc::EEXIST);
        let mapped = map_rename_error(&error, "a.rs", "b.rs");
        assert_eq!(mapped.code(), ErrorCode::DestinationExists);
    }

    #[test]
    fn a_cross_device_move_is_still_a_refusal_when_it_is_an_errno() {
        let error = Error::from_raw_os_error(libc::EXDEV);
        let mapped = map_rename_error(&error, "a.rs", "/other/a.rs");
        assert_eq!(mapped.code(), ErrorCode::UnsupportedOperation);
    }

    #[test]
    fn something_the_engine_cannot_explain_stays_a_fault() {
        // The distinction only means anything if it can still say "fault".
        let error = Error::from_raw_os_error(libc::EIO);
        let mapped = map_rename_error(&error, "a.rs", "b.rs");
        assert!(
            matches!(mapped, safescope::error::Error::Faulted(_)),
            "an I/O failure is not a rule refusing"
        );
    }
}
