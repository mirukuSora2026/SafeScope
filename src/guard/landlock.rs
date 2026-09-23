//! The Linux half of the guard: Landlock.
//!
//! Landlock lets an unprivileged process give up filesystem access for itself
//! and everything it goes on to start, irreversibly. That is exactly the shape
//! the guard needs, and unlike a mount namespace it asks for no privilege the
//! person running it may not have.
//!
//! # Saying "everything but this" in a language with no "but"
//!
//! A Landlock ruleset grants rights on path hierarchies and denies whatever it
//! was not granted. There is no deny rule, so a hole cannot be punched in a
//! broad grant — writing "allow everything, then take the workspace back" is not
//! expressible.
//!
//! It is said the other way round instead. Read is granted on `/`, so the whole
//! filesystem stays readable. Write is granted on the *siblings* of each step
//! down to the workspace: everything in `/` except the first component of its
//! path, everything in that except the second, and so on. Rights are the union
//! of what matched, so every directory that is not on the way to the workspace
//! keeps write access, and the workspace itself is left with only the read that
//! `/` gave it.
//!
//! A path that cannot be read while building the list is skipped, which loses
//! write access to it rather than granting it — a rule that fails to be written
//! must fail closed.

use std::ffi::CString;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd, RawFd};
use std::path::Path;

/// `struct landlock_ruleset_attr`.
#[repr(C)]
struct RulesetAttr {
    handled_access_fs: u64,
    handled_access_net: u64,
    scoped: u64,
}

/// `struct landlock_path_beneath_attr`, which is packed in the kernel headers.
#[repr(C, packed)]
struct PathBeneathAttr {
    allowed_access: u64,
    parent_fd: i32,
}

const LANDLOCK_CREATE_RULESET_VERSION: u32 = 1 << 0;
const LANDLOCK_RULE_PATH_BENEATH: u32 = 1;

// The access bits, lowest ABI first. Everything above ABI 1 is masked out when
// the running kernel is older, because a ruleset naming a right the kernel does
// not know is refused outright.
const EXECUTE: u64 = 1 << 0;
const WRITE_FILE: u64 = 1 << 1;
const READ_FILE: u64 = 1 << 2;
const READ_DIR: u64 = 1 << 3;
const REMOVE_DIR: u64 = 1 << 4;
const REMOVE_FILE: u64 = 1 << 5;
const MAKE_CHAR: u64 = 1 << 6;
const MAKE_DIR: u64 = 1 << 7;
const MAKE_REG: u64 = 1 << 8;
const MAKE_SOCK: u64 = 1 << 9;
const MAKE_FIFO: u64 = 1 << 10;
const MAKE_BLOCK: u64 = 1 << 11;
const MAKE_SYM: u64 = 1 << 12;
const REFER: u64 = 1 << 13;
const TRUNCATE: u64 = 1 << 14;
const IOCTL_DEV: u64 = 1 << 15;

/// Reading, which the workspace keeps.
const READ_RIGHTS: u64 = EXECUTE | READ_FILE | READ_DIR;

/// Everything that changes a directory or a file, which the workspace loses.
const WRITE_RIGHTS: u64 = WRITE_FILE
    | REMOVE_DIR
    | REMOVE_FILE
    | MAKE_CHAR
    | MAKE_DIR
    | MAKE_REG
    | MAKE_SOCK
    | MAKE_FIFO
    | MAKE_BLOCK
    | MAKE_SYM
    | REFER
    | TRUNCATE
    | IOCTL_DEV;

/// The rights each ABI level understands.
///
/// `REFER` arrived in 2, `TRUNCATE` in 3 and `IOCTL_DEV` in 5. Handing an older
/// kernel a bit it does not know makes it refuse the whole ruleset, so the mask
/// is narrowed to what the kernel in front of us actually has.
fn rights_for(abi: i32) -> u64 {
    let mut mask = READ_RIGHTS | WRITE_RIGHTS;
    if abi < 5 {
        mask &= !IOCTL_DEV;
    }
    if abi < 3 {
        mask &= !TRUNCATE;
    }
    if abi < 2 {
        mask &= !REFER;
    }
    mask
}

/// The Landlock ABI this kernel speaks, or `None` where there is no Landlock.
pub fn abi_version() -> Option<i32> {
    // SAFETY: the version query takes a null attribute and a zero size by
    // definition; it reads nothing from this process.
    let version = unsafe {
        libc::syscall(
            libc::SYS_landlock_create_ruleset,
            std::ptr::null::<RulesetAttr>(),
            0usize,
            LANDLOCK_CREATE_RULESET_VERSION,
        )
    };
    (version > 0).then_some(version as i32)
}

/// Builds a ruleset that permits everything except writing inside `workspace`.
///
/// Returns the ruleset descriptor. Applying it is [`restrict_current_thread`],
/// which happens in the child between fork and exec — restricting here would
/// take the workspace away from the engine, which is the one process that must
/// keep it.
pub fn build(workspace: &Path) -> std::io::Result<OwnedFd> {
    let abi = abi_version().ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "this kernel has no Landlock",
        )
    })?;
    let handled = rights_for(abi);

    let attr = RulesetAttr {
        handled_access_fs: handled,
        handled_access_net: 0,
        scoped: 0,
    };
    // SAFETY: `attr` is live and its size is passed alongside it.
    let ruleset = unsafe {
        libc::syscall(
            libc::SYS_landlock_create_ruleset,
            &raw const attr,
            std::mem::size_of::<RulesetAttr>(),
            0u32,
        )
    };
    if ruleset < 0 {
        return Err(std::io::Error::last_os_error());
    }
    // SAFETY: the syscall returned a fresh descriptor this function now owns.
    let ruleset = unsafe { OwnedFd::from_raw_fd(ruleset as RawFd) };

    // The whole filesystem stays readable, including the workspace: an agent
    // that cannot read the code cannot change it correctly either.
    allow(ruleset.as_raw_fd(), Path::new("/"), READ_RIGHTS & handled)?;

    // Write, granted to everything that is not on the way to the workspace.
    for (directory, keep) in siblings_along(workspace) {
        let Ok(entries) = std::fs::read_dir(&directory) else {
            // Unreadable: nothing is granted, so it stays unwritable. A rule
            // that could not be written must fail closed.
            continue;
        };
        for entry in entries.flatten() {
            if entry.file_name() == keep {
                continue;
            }
            // A missing or racing entry is skipped for the same reason.
            let _ = allow(ruleset.as_raw_fd(), &entry.path(), handled);
        }
    }

    Ok(ruleset)
}

/// Each directory on the way to `workspace`, with the component to leave out.
///
/// For `/a/b/c` that is `("/", "a")`, `("/a", "b")`, `("/a/b", "c")`. Granting
/// write to everything else in each of them leaves exactly one path through the
/// tree without it.
fn siblings_along(workspace: &Path) -> Vec<(std::path::PathBuf, std::ffi::OsString)> {
    let mut steps = Vec::new();
    let mut current = std::path::PathBuf::from("/");
    let components: Vec<_> = workspace
        .components()
        .filter_map(|component| match component {
            std::path::Component::Normal(name) => Some(name.to_os_string()),
            _ => None,
        })
        .collect();

    for name in components {
        steps.push((current.clone(), name.clone()));
        current.push(&name);
    }
    steps
}

/// Grants `rights` on everything beneath `path`.
fn allow(ruleset: RawFd, path: &Path, rights: u64) -> std::io::Result<()> {
    if rights == 0 {
        return Ok(());
    }
    let name = CString::new(path.as_os_str().as_encoded_bytes())
        .map_err(|_| std::io::Error::from(std::io::ErrorKind::InvalidInput))?;

    // O_PATH is enough to name a hierarchy, and unlike a read handle it works on
    // things this process may not open for reading.
    // SAFETY: `name` is NUL-terminated and outlives the call.
    let handle = unsafe { libc::open(name.as_ptr(), libc::O_PATH | libc::O_CLOEXEC) };
    if handle < 0 {
        return Err(std::io::Error::last_os_error());
    }
    // SAFETY: `open` returned a descriptor this owns and closes on drop.
    let handle = unsafe { OwnedFd::from_raw_fd(handle) };

    let rule = PathBeneathAttr {
        allowed_access: rights,
        parent_fd: handle.as_raw_fd(),
    };
    // SAFETY: `rule` is live for the call and matches the rule type given.
    let outcome = unsafe {
        libc::syscall(
            libc::SYS_landlock_add_rule,
            ruleset,
            LANDLOCK_RULE_PATH_BENEATH,
            &raw const rule,
            0u32,
        )
    };
    if outcome != 0 {
        return Err(std::io::Error::last_os_error());
    }
    Ok(())
}

/// Applies `ruleset` to this thread and everything it goes on to exec.
///
/// Called between fork and exec, where only async-signal-safe work is allowed —
/// so it is two raw syscalls and no allocation. The ruleset was built before the
/// fork for exactly that reason.
///
/// # Safety
///
/// Must be called in a child process between `fork` and `exec`, with `ruleset`
/// a descriptor from [`build`] that is still open.
pub unsafe fn restrict_current_thread(ruleset: RawFd) -> std::io::Result<()> {
    // Landlock refuses to restrict a process that could still gain privileges
    // through a set-uid binary; without this the next call fails.
    if unsafe { libc::prctl(libc::PR_SET_NO_NEW_PRIVS, 1, 0, 0, 0) } != 0 {
        return Err(std::io::Error::last_os_error());
    }
    if unsafe { libc::syscall(libc::SYS_landlock_restrict_self, ruleset, 0u32) } != 0 {
        return Err(std::io::Error::last_os_error());
    }
    Ok(())
}
