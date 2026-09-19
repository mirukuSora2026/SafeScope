//! Recovery data.
//!
//! The property worth testing here is not that a write happened but that what
//! comes back is what went in, and that a snapshot which has since been damaged
//! is reported rather than handed out as if it were good.

use std::fs;

use safescope::error::ErrorCode;
use safescope::hash::ContentHash;
use safescope::ids::WorkspaceId;
use safescope::store::snapshot::SnapshotStore;
use safescope::store::{DATA_DIR_ENV, StatePaths};
use tempfile::TempDir;

fn env_lock() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn store() -> (TempDir, TempDir, SnapshotStore) {
    let data = TempDir::new().expect("data");
    let workspace = TempDir::new().expect("workspace");
    unsafe { std::env::set_var(DATA_DIR_ENV, data.path()) };
    let paths = StatePaths::for_workspace(WorkspaceId::new(), workspace.path()).expect("paths");
    paths.create().expect("create");
    let store = SnapshotStore::new(&paths);
    (data, workspace, store)
}

#[test]
fn stored_contents_come_back_unchanged() {
    let _guard = env_lock();
    let (_data, _workspace, store) = store();

    let hash = store.store(b"class Login {}\n").expect("store");
    assert_eq!(hash, ContentHash::of_bytes(b"class Login {}\n"));
    assert_eq!(store.read(hash).expect("read"), b"class Login {}\n");
}

#[test]
fn identical_contents_are_stored_once() {
    let _guard = env_lock();
    let (_data, _workspace, store) = store();

    let first = store.store(b"same").expect("first");
    let usage = store.usage_bytes().expect("usage");
    let second = store.store(b"same").expect("second");

    assert_eq!(first, second);
    assert_eq!(store.usage_bytes().expect("usage"), usage, "no second copy");
}

#[test]
fn different_contents_are_stored_separately() {
    let _guard = env_lock();
    let (_data, _workspace, store) = store();

    let a = store.store(b"one").expect("a");
    let b = store.store(b"two").expect("b");
    assert_ne!(a, b);
    assert_eq!(store.read(a).unwrap(), b"one");
    assert_eq!(store.read(b).unwrap(), b"two");
}

#[test]
fn an_empty_file_is_stored_like_any_other() {
    let _guard = env_lock();
    let (_data, _workspace, store) = store();

    let hash = store.store(b"").expect("store");
    assert!(store.verify(hash).expect("verify"));
    assert_eq!(store.read(hash).expect("read"), b"");
}

#[test]
fn a_large_file_round_trips() {
    let _guard = env_lock();
    let (_data, _workspace, store) = store();

    let contents = vec![b'x'; 4 * 1024 * 1024 + 11];
    let hash = store.store(&contents).expect("store");
    assert_eq!(store.read(hash).expect("read"), contents);
}

#[test]
fn a_damaged_snapshot_is_reported_rather_than_returned() {
    // Handing back a snapshot that no longer hashes to its own name would undo
    // a change into corruption.
    let _guard = env_lock();
    let (_data, _workspace, store) = store();

    let hash = store.store(b"original").expect("store");
    let path = store.directory().join(hash.shard()).join(hash.to_hex());
    fs::write(&path, b"tampered").expect("damage the snapshot");

    assert!(
        !store.verify(hash).expect("verify"),
        "verification catches it"
    );
    let error = store.read(hash).unwrap_err();
    assert_eq!(error.code(), ErrorCode::SnapshotFailed);
}

#[test]
fn a_damaged_snapshot_is_replaced_on_the_next_store() {
    // Deduplication must not trust a name. A corrupted file whose name happens
    // to match would otherwise satisfy the very check that guarantees undo.
    let _guard = env_lock();
    let (_data, _workspace, store) = store();

    let hash = store.store(b"original").expect("store");
    let path = store.directory().join(hash.shard()).join(hash.to_hex());
    fs::write(&path, b"tampered").expect("damage");

    let again = store.store(b"original").expect("store again");
    assert_eq!(again, hash);
    assert!(store.verify(hash).expect("verify"), "the good copy is back");
    assert_eq!(store.read(hash).expect("read"), b"original");
}

#[test]
fn a_missing_snapshot_is_reported() {
    let _guard = env_lock();
    let (_data, _workspace, store) = store();

    let never_stored = ContentHash::of_bytes(b"never stored");
    assert!(!store.contains(never_stored));
    assert!(!store.verify(never_stored).expect("verify"));

    let error = store.read(never_stored).unwrap_err();
    assert_eq!(error.code(), ErrorCode::SnapshotFailed);
}

#[test]
fn usage_grows_with_stored_content() {
    let _guard = env_lock();
    let (_data, _workspace, store) = store();

    assert_eq!(store.usage_bytes().expect("usage"), 0);
    store.store(&vec![b'a'; 1000]).expect("store");
    assert_eq!(store.usage_bytes().expect("usage"), 1000);
    store.store(&vec![b'b'; 500]).expect("store");
    assert_eq!(store.usage_bytes().expect("usage"), 1500);
}

#[test]
fn snapshots_are_sharded() {
    let _guard = env_lock();
    let (_data, _workspace, store) = store();

    let hash = store.store(b"content").expect("store");
    let shard = store.directory().join(hash.shard());
    assert!(
        shard.is_dir(),
        "one directory must not accumulate every snapshot"
    );
    assert!(shard.join(hash.to_hex()).is_file());
}

#[test]
fn storing_leaves_no_temporary_behind() {
    let _guard = env_lock();
    let (_data, _workspace, store) = store();

    let hash = store.store(b"content").expect("store");
    let leftovers: Vec<_> = fs::read_dir(store.directory().join(hash.shard()))
        .expect("read shard")
        .filter_map(Result::ok)
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| name.starts_with(".sfs-tmp-"))
        .collect();
    assert!(leftovers.is_empty(), "left behind: {leftovers:?}");
}
