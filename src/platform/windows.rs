//! What Windows offers in place of the calls the rest of the engine is built on.
//!
//! Two of the three have a genuine equivalent and one does not, and saying which
//! is which is the point of this file.
//!
//! **Renaming without overwriting** does. `NtSetInformationFile` with
//! `FileRenameInformation` takes a `RootDirectory` handle and a `ReplaceIfExists`
//! flag, so the rename is both relative to a directory handle — which is what
//! keeps path decisions and execution on the same directory, as everywhere
//! else — and refused atomically when the destination is taken. It is not a
//! check followed by a rename, which is the emulation this crate refuses
//! everywhere.
//!
//! The NT call rather than the Win32 one, measured: `SetFileInformationByHandle`
//! documents the same structure with the same `RootDirectory` field, and refuses
//! any non-null value for it with ERROR_INVALID_PARAMETER. A probe on the CI
//! runner tried both against one directory handle — the Win32 call failed, the
//! NT call renamed, and the Win32 call with a full path and no handle renamed
//! too. That last is the one thing this file must not do: a full path is
//! resolved again at the moment of the rename, which is the gap I4 closes.
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
use windows_sys::Wdk::Storage::FileSystem::{
    FILE_RENAME_INFORMATION, FileRenameInformation, NtSetInformationFile,
};
use windows_sys::Win32::Foundation::{
    ERROR_ALREADY_EXISTS, ERROR_FILE_EXISTS, ERROR_LOCK_VIOLATION, ERROR_NOT_SAME_DEVICE,
    ERROR_SHARING_VIOLATION, HANDLE, RtlNtStatusToDosError,
};
use windows_sys::Win32::Storage::FileSystem::{
    LOCKFILE_EXCLUSIVE_LOCK, LOCKFILE_FAIL_IMMEDIATELY, LockFileEx,
};
use windows_sys::Win32::System::IO::{IO_STATUS_BLOCK, OVERLAPPED};

/// What a rename needs on the handle of the file being renamed.
///
/// `DELETE` to move the name, and `SYNCHRONIZE` so the handle is a synchronous
/// one and the NT call has finished when it returns rather than merely started.
///
/// Both are needed explicitly: `cap-std` treats `access_mode` as the whole mask
/// rather than as something to combine with `read`, so whatever is not named
/// here is not granted.
const RENAME_ACCESS: u32 = 0x0001_0000 | 0x0010_0000;

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
            .access_mode(RENAME_ACCESS)
            .share_mode(SHARE_ALL);
    }
    let file = from_directory.open_with(from_name, &options)?;

    // FILE_RENAME_INFORMATION ends in a variable-length name, so it is built in a byte
    // buffer rather than as a value: the struct's own size describes only the
    // first character of it.
    //
    // The name is NUL-terminated and the length excludes the terminator, which
    // is what the documentation asks for and reads like a contradiction until
    // both halves are read together: "FileName — a NUL-terminated wide-character
    // string", "FileNameLength — the size of FileName in bytes; a terminating
    // null character is not required".
    let mut name: Vec<u16> = Path::new(to_name).as_os_str().encode_wide().collect();
    let name_bytes = name.len() * std::mem::size_of::<u16>();
    name.push(0);

    // `FileName[1]` already reserves one character inside the struct, so the
    // terminator lands inside the space `size_of` accounts for.
    // Aligned for the struct, which a `Vec<u8>` does not promise: its buffer is
    // guaranteed only byte alignment, and this one holds a HANDLE. A vector of
    // the aligned element gives the guarantee for free.
    let header = std::mem::size_of::<FILE_RENAME_INFORMATION>();
    let words = (header + name_bytes).div_ceil(std::mem::size_of::<u64>());
    let mut buffer: Vec<u64> = vec![0; words];

    // SAFETY: `buffer` is at least `header + name_bytes` bytes and aligned to
    // eight, which is what FILE_RENAME_INFORMATION needs for the handle it
    // carries.
    let info = buffer.as_mut_ptr().cast::<FILE_RENAME_INFORMATION>();
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
    // the buffer is live and of the length passed, and the status block is a
    // zeroed value of the right type. The handle is synchronous, so the call
    // has finished with both when it returns.
    let mut status: IO_STATUS_BLOCK = unsafe { std::mem::zeroed() };
    let outcome = unsafe {
        NtSetInformationFile(
            file.as_raw_handle() as HANDLE,
            &raw mut status,
            buffer.as_ptr().cast(),
            (words * std::mem::size_of::<u64>()) as u32,
            FileRenameInformation,
        )
    };
    if outcome < 0 {
        // An NTSTATUS, turned into the Win32 code the rest of the engine and
        // `std::io::Error` understand: a taken destination arrives as
        // STATUS_OBJECT_NAME_COLLISION and leaves as ERROR_ALREADY_EXISTS.
        // SAFETY: a pure lookup on a value.
        let code = unsafe { RtlNtStatusToDosError(outcome) };
        return Err(translate(std::io::Error::from_raw_os_error(code as i32)));
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
