//! What Windows offers in place of the calls the rest of the engine is built on.
//!
//! Two of the three have a genuine equivalent and one does not, and saying which
//! is which is the point of this file.
//!
//! **Renaming without overwriting** does. `SetFileInformationByHandle` with
//! `FileRenameInfo` takes a `RootDirectory` handle and a `ReplaceIfExists` flag,
//! so the rename is both relative to a directory handle — which is what keeps
//! path decisions and execution on the same directory, as everywhere else — and
//! refused atomically when the destination is taken. It is not a check followed
//! by a rename, which is the emulation this crate refuses everywhere.
//!
//! **A single-writer lock** does. `LockFileEx` with `LOCKFILE_EXCLUSIVE_LOCK`
//! and `LOCKFILE_FAIL_IMMEDIATELY` refuses rather than waits, and the lock goes
//! when the handle does, including when the process dies.
//!
//! **Flushing a directory does not.** Windows has no equivalent of `fsync` on a
//! directory: `FlushFileBuffers` wants a handle opened for writing and a
//! directory cannot be. The consequence is stated rather than papered over in
//! [`flush_directory`].

use std::os::windows::ffi::OsStrExt as _;
use std::os::windows::io::{AsHandle, AsRawHandle, BorrowedHandle};
use std::path::Path;

use cap_std::fs::Dir;
use windows_sys::Win32::Foundation::{
    ERROR_ALREADY_EXISTS, ERROR_FILE_EXISTS, ERROR_LOCK_VIOLATION, ERROR_NOT_SAME_DEVICE,
    ERROR_SHARING_VIOLATION, HANDLE,
};
use windows_sys::Win32::Storage::FileSystem::{
    FILE_RENAME_INFO, FileRenameInfo, LOCKFILE_EXCLUSIVE_LOCK, LOCKFILE_FAIL_IMMEDIATELY,
    LockFileEx, SetFileInformationByHandle,
};
use windows_sys::Win32::System::IO::OVERLAPPED;

/// `DELETE`, which is the access a rename needs on the file being renamed.
const DELETE_ACCESS: u32 = 0x0001_0000;

/// Everything, so a rename does not fail because something else has it open.
///
/// Matching Unix, where a rename is not blocked by a reader. A narrower share
/// mode would make this fail in cases the rest of the engine treats as normal.
const SHARE_ALL: u32 = 0x0000_0001 | 0x0000_0002 | 0x0000_0004;

/// Renames relative to two directory handles, refusing to overwrite.
///
/// `ReplaceIfExists` is false and the kernel enforces it, so there is no window
/// between deciding the destination is free and taking it.
pub fn rename_no_replace(
    from_directory: &Dir,
    from_name: &str,
    to_directory: &Dir,
    to_name: &str,
) -> std::io::Result<()> {
    // `DELETE` rather than read or write: renaming is an operation on the name,
    // not the contents, and asking for more would fail on files held open
    // elsewhere for reasons that have nothing to do with this.
    let mut options = cap_std::fs::OpenOptions::new();
    {
        use cap_std::fs::OpenOptionsExt as _;
        options
            .read(true)
            .access_mode(DELETE_ACCESS)
            .share_mode(SHARE_ALL);
    }
    let file = from_directory.open_with(from_name, &options)?;

    // FILE_RENAME_INFO ends in a variable-length name, so it is built in a byte
    // buffer rather than as a value: the struct's own size describes only the
    // first character of it.
    let name: Vec<u16> = Path::new(to_name)
        .as_os_str()
        .encode_wide()
        .collect::<Vec<u16>>();
    let name_bytes = name.len() * std::mem::size_of::<u16>();
    let header = std::mem::size_of::<FILE_RENAME_INFO>();
    let mut buffer = vec![0u8; header + name_bytes];

    // SAFETY: `buffer` is at least `header` bytes and correctly aligned for
    // FILE_RENAME_INFO, because it is the only thing written at offset zero.
    let info = buffer.as_mut_ptr().cast::<FILE_RENAME_INFO>();
    unsafe {
        (*info).Anonymous.ReplaceIfExists = false;
        (*info).RootDirectory = to_directory.as_raw_handle() as HANDLE;
        (*info).FileNameLength = name_bytes as u32;
        // The name follows the header. `FileName` is declared as one character,
        // so it is written through a pointer to the whole run rather than as a
        // field assignment.
        std::ptr::copy_nonoverlapping(
            name.as_ptr(),
            std::ptr::addr_of_mut!((*info).FileName).cast::<u16>(),
            name.len(),
        );
    }

    // SAFETY: the handle is borrowed from `file` for the duration of the call,
    // and the buffer is live and of the length passed.
    let outcome = unsafe {
        SetFileInformationByHandle(
            file.as_raw_handle() as HANDLE,
            FileRenameInfo,
            buffer.as_ptr().cast(),
            buffer.len() as u32,
        )
    };
    if outcome == 0 {
        return Err(translate(std::io::Error::last_os_error()));
    }
    Ok(())
}

/// Reports a taken destination and a cross-volume move the way Unix does.
///
/// The rest of the engine decides what to say from `AlreadyExists` and the
/// cross-device error; without this, Windows would report the first as a
/// sharing violation and the caller would call it a fault rather than a refusal.
fn translate(error: std::io::Error) -> std::io::Error {
    let code = error.raw_os_error().unwrap_or_default() as u32;
    if code == ERROR_ALREADY_EXISTS || code == ERROR_FILE_EXISTS {
        return std::io::Error::new(std::io::ErrorKind::AlreadyExists, error);
    }
    if code == ERROR_SHARING_VIOLATION || code == ERROR_LOCK_VIOLATION {
        // The destination name is taken by something held open. It is still a
        // destination that cannot be written, which is what the caller acts on.
        return std::io::Error::new(std::io::ErrorKind::AlreadyExists, error);
    }
    if code == ERROR_NOT_SAME_DEVICE {
        // The kind, not a POSIX number. `from_raw_os_error` on Windows reads its
        // argument as a Win32 code, so EXDEV's 18 would have arrived as "there
        // are no more files" — a match by coincidence carrying the wrong
        // sentence for anybody who read it.
        return std::io::Error::new(std::io::ErrorKind::CrossesDevices, error);
    }
    error
}

/// Makes a directory's own metadata durable — which Windows cannot do.
///
/// There is no directory flush here. `FlushFileBuffers` needs write access and a
/// directory handle has none, and no other call orders a rename against the
/// data written before it.
///
/// What this costs, precisely: a crash immediately after a rename returns can
/// lose that rename, because the file data was flushed but the name change may
/// still be only in the filesystem's journal. It cannot produce a half-written
/// file — the contents are flushed before the rename, as everywhere else — so
/// the loss is of a completed operation, not a corrupted one.
///
/// The journal records an operation before it runs, so recovery finds one whose
/// recorded state is "applying" and whose workspace still matches the state from
/// before. That is a case it already classifies and reports rather than guesses
/// at. The guarantee is weaker than on Unix, and [`super::rename_is_durable`] is
/// false here so `safescope doctor` says so to the person who has to decide
/// whether to trust what is on disk.
pub fn flush_directory(_directory: &Dir) -> std::io::Result<()> {
    Ok(())
}

/// Takes an exclusive lock on an open file, or reports it is already held.
///
/// Never waits, matching the Unix path: what it would wait for is another
/// session that may be sitting at a prompt.
pub fn try_lock_exclusive(handle: BorrowedHandle<'_>) -> std::io::Result<()> {
    let mut overlapped: OVERLAPPED = unsafe { std::mem::zeroed() };

    // SAFETY: the handle is borrowed for the call and `overlapped` is a live,
    // zeroed structure of the right type.
    let outcome = unsafe {
        LockFileEx(
            handle.as_raw_handle() as HANDLE,
            LOCKFILE_EXCLUSIVE_LOCK | LOCKFILE_FAIL_IMMEDIATELY,
            0,
            u32::MAX,
            u32::MAX,
            &raw mut overlapped,
        )
    };
    if outcome == 0 {
        return Err(std::io::Error::last_os_error());
    }
    Ok(())
}

/// Whether an error from [`try_lock_exclusive`] means somebody else holds it.
pub fn is_already_locked(error: &std::io::Error) -> bool {
    let code = error.raw_os_error().unwrap_or_default() as u32;
    code == ERROR_LOCK_VIOLATION || code == ERROR_SHARING_VIOLATION
}

/// Borrows a handle from anything that has one, for the lock call.
pub fn handle_of(file: &std::fs::File) -> BorrowedHandle<'_> {
    file.as_handle()
}
