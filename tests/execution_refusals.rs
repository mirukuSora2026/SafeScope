//! What the engine refuses to do, and the proof that it changed nothing.
//!
//! Each of these asserts twice: that the right refusal came back, and that the
//! workspace is exactly as it was. A refusal that had already half-happened
//! would be worse than no check at all.

mod harness;

use std::fs;
use std::time::{Duration, SystemTime};

use harness::{Harness, env_lock, path};
use safescope::error::ErrorCode;
use safescope::executor::RequestKey;
use safescope::hash::ContentHash;
use safescope::ids::RequestId;
use safescope::planner::ChangeRequest;

#[test]
fn create_refuses_an_existing_file() {
    let _guard = env_lock();
    let harness = Harness::new();
    harness.write("src/auth/Login.java", "already here\n");

    let error = harness
        .plan(ChangeRequest::Create {
            path: path("src/auth/Login.java"),
            contents: b"replacement\n".to_vec(),
        })
        .unwrap_err();

    assert_eq!(error.code(), ErrorCode::DestinationExists);
    assert_eq!(harness.read("src/auth/Login.java"), "already here\n");
}

#[test]
fn replace_refuses_a_missing_file() {
    let _guard = env_lock();
    let harness = Harness::new();

    let error = harness
        .plan(ChangeRequest::Replace {
            path: path("src/auth/Nothing.java"),
            contents: b"contents\n".to_vec(),
        })
        .unwrap_err();
    assert_eq!(error.code(), ErrorCode::TargetMissing);
}

#[test]
fn a_move_refuses_an_occupied_destination() {
    let _guard = env_lock();
    let harness = Harness::new();
    harness.write("src/auth/A.java", "source\n");
    harness.write("src/auth/B.java", "destination\n");

    let error = harness
        .plan(ChangeRequest::Move {
            from: path("src/auth/A.java"),
            to: path("src/auth/B.java"),
        })
        .unwrap_err();

    assert_eq!(error.code(), ErrorCode::DestinationExists);
    assert_eq!(harness.read("src/auth/B.java"), "destination\n");
}

#[test]
fn a_plan_is_refused_once_the_file_has_moved_on() {
    // I3. Applying a plan built against different contents would overwrite
    // whatever arrived in between.
    let _guard = env_lock();
    let mut harness = Harness::new();
    harness.write("src/auth/Login.java", "planned against this\n");

    let plan = harness
        .plan(ChangeRequest::Replace {
            path: path("src/auth/Login.java"),
            contents: b"the change\n".to_vec(),
        })
        .expect("plan");

    harness.write("src/auth/Login.java", "someone else edited it\n");
    let error = harness.apply(&plan, None).unwrap_err();

    assert_eq!(error.code(), ErrorCode::SourceChanged);
    assert_eq!(
        harness.read("src/auth/Login.java"),
        "someone else edited it\n",
        "the other edit survives"
    );
}

#[test]
fn an_expired_plan_is_refused() {
    let _guard = env_lock();
    let mut harness = Harness::new();
    harness.write("src/auth/Login.java", "old\n");

    let mut plan = harness
        .plan(ChangeRequest::Replace {
            path: path("src/auth/Login.java"),
            contents: b"new\n".to_vec(),
        })
        .expect("plan");
    plan.expires_at = SystemTime::now() - Duration::from_secs(1);

    let error = harness.apply(&plan, None).unwrap_err();
    assert_eq!(error.code(), ErrorCode::PlanExpired);
    assert_eq!(harness.read("src/auth/Login.java"), "old\n");
}

#[test]
fn a_resent_request_does_not_apply_the_change_twice() {
    let _guard = env_lock();
    let mut harness = Harness::new();
    let key = RequestKey {
        id: RequestId::new(),
        digest: ContentHash::of_bytes(b"the request"),
    };

    let plan = harness
        .plan(ChangeRequest::Create {
            path: path("src/auth/Once.java"),
            contents: b"once\n".to_vec(),
        })
        .expect("plan");

    let first = harness.apply(&plan, Some(key)).expect("first");
    let second = harness.apply(&plan, Some(key)).expect("resend");

    assert_eq!(first.id, second.id, "the original result comes back");
    assert_eq!(
        harness
            .journal
            .history(harness.task)
            .expect("history")
            .len(),
        1
    );
}

#[test]
fn the_same_key_with_a_different_request_is_refused() {
    let _guard = env_lock();
    let mut harness = Harness::new();
    let id = RequestId::new();

    let plan = harness
        .plan(ChangeRequest::Create {
            path: path("src/auth/Once.java"),
            contents: b"once\n".to_vec(),
        })
        .expect("plan");
    harness
        .apply(
            &plan,
            Some(RequestKey {
                id,
                digest: ContentHash::of_bytes(b"first"),
            }),
        )
        .expect("first");

    let other = harness
        .plan(ChangeRequest::Create {
            path: path("src/auth/Twice.java"),
            contents: b"twice\n".to_vec(),
        })
        .expect("plan");
    let error = harness
        .apply(
            &other,
            Some(RequestKey {
                id,
                digest: ContentHash::of_bytes(b"second"),
            }),
        )
        .unwrap_err();

    assert_eq!(error.code(), ErrorCode::RequestMismatch);
    assert!(!harness.exists("src/auth/Twice.java"));
}

#[test]
fn a_path_outside_the_allowed_scope_never_reaches_the_filesystem() {
    let _guard = env_lock();
    let harness = Harness::new();
    fs::create_dir_all(harness.root.path().join("docs")).expect("mkdir");

    let error = harness
        .plan(ChangeRequest::Create {
            path: path("docs/Notes.md"),
            contents: b"notes\n".to_vec(),
        })
        .unwrap_err();

    assert_eq!(error.code(), ErrorCode::ScopeDenied);
    assert!(!harness.exists("docs/Notes.md"));
}

#[test]
fn a_denied_path_never_reaches_the_filesystem() {
    let _guard = env_lock();
    let harness = Harness::new();

    let error = harness
        .plan(ChangeRequest::Create {
            path: path("src/.env"),
            contents: b"SECRET=1\n".to_vec(),
        })
        .unwrap_err();

    assert_eq!(error.code(), ErrorCode::ScopeDenied);
    assert!(!harness.exists("src/.env"));
}

#[test]
fn a_file_over_the_policy_limit_is_refused() {
    let _guard = env_lock();
    let harness = Harness::new();
    let limit = harness.policy.budget().max_file_bytes;

    let error = harness
        .plan(ChangeRequest::Create {
            path: path("src/auth/Huge.java"),
            contents: vec![b'x'; (limit + 1) as usize],
        })
        .unwrap_err();

    assert_eq!(error.code(), ErrorCode::FileTooLarge);
    assert!(!harness.exists("src/auth/Huge.java"));
}

#[test]
fn planning_alone_changes_nothing_in_the_workspace() {
    let _guard = env_lock();
    let harness = Harness::new();
    harness.write("src/auth/Login.java", "untouched\n");

    harness
        .plan(ChangeRequest::Replace {
            path: path("src/auth/Login.java"),
            contents: b"planned\n".to_vec(),
        })
        .expect("plan");
    harness
        .plan(ChangeRequest::Trash {
            path: path("src/auth/Login.java"),
        })
        .expect("plan");

    assert_eq!(harness.read("src/auth/Login.java"), "untouched\n");
}
