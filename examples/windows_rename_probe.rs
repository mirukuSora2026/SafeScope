//! Which way of renaming works on this Windows, and which does not.
//!
//! Three rounds of CI narrowed the failure to one variable and could not settle
//! it, because every round costs a push and answers one guess. This settles it
//! in one run on a real machine: it tries each candidate against the same
//! conditions the engine uses — a `cap-std` directory handle, a relative target
//! name — and prints what each one returned.
//!
//! Run it and paste the output:
//!
//! ```text
//! cargo run --example windows_rename_probe
//! ```
//!
//! It writes only inside a temporary directory it makes for itself, and removes
//! it on the way out. Nothing here is part of the engine; delete this file once
//! the answer is known.

fn main() {
    #[cfg(not(windows))]
    println!("This probe only means anything on Windows.");

    #[cfg(windows)]
    windows::run();
}

#[cfg(windows)]
mod windows {
    use std::os::windows::ffi::OsStrExt as _;
    use std::os::windows::io::AsRawHandle as _;
    use std::path::Path;

    use cap_std::fs::{Dir, OpenOptions, OpenOptionsExt as _};
    use windows_sys::Wdk::Storage::FileSystem::{FILE_RENAME_INFORMATION, NtSetInformationFile};
    use windows_sys::Win32::Foundation::HANDLE;
    use windows_sys::Win32::Storage::FileSystem::{
        FILE_RENAME_INFO, FileRenameInfo, SetFileInformationByHandle,
    };
    use windows_sys::Win32::System::IO::IO_STATUS_BLOCK;

    const DELETE_AND_SYNCHRONIZE: u32 = 0x0001_0000 | 0x0010_0000;
    /// `FileRenameInformation` in the NT information class enumeration.
    const FILE_RENAME_INFORMATION_CLASS: i32 = 10;

    pub fn run() {
        let root = std::env::temp_dir().join(format!("sfs-probe-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("make the probe directory");

        println!("probe directory: {}", root.display());
        println!();

        report(
            "A  SetFileInformationByHandle, RootDirectory = the directory handle",
            {
                let (directory, from, to) = prepare(&root, "a");
                set_file_information(&directory, &from, Some(&directory), &to)
            },
        );

        report(
            "B  SetFileInformationByHandle, RootDirectory = NULL, full path",
            {
                let (directory, from, _) = prepare(&root, "b");
                let full = root.join("b-dir").join("b-after.txt");
                set_file_information(&directory, &from, None, &full.to_string_lossy())
            },
        );

        report(
            "C  NtSetInformationFile, RootDirectory = the directory handle",
            {
                let (directory, from, to) = prepare(&root, "c");
                nt_set_information(&directory, &from, &directory, &to)
            },
        );

        println!();
        println!("A is what the engine does today. Whichever of these says `ok` is the answer.");
        let _ = std::fs::remove_dir_all(&root);
    }

    /// A directory holding one file, opened the way the engine opens them.
    fn prepare(root: &Path, tag: &str) -> (Dir, String, String) {
        let directory = root.join(format!("{tag}-dir"));
        std::fs::create_dir_all(&directory).expect("make a directory");
        let from = format!("{tag}-before.txt");
        std::fs::write(directory.join(&from), b"contents").expect("write a file");

        let handle = Dir::open_ambient_dir(&directory, cap_std::ambient_authority())
            .expect("open the directory");
        (handle, from, format!("{tag}-after.txt"))
    }

    /// Opens the file to be renamed with the access a rename needs.
    fn open_source(directory: &Dir, name: &str) -> std::io::Result<cap_std::fs::File> {
        let mut options = OpenOptions::new();
        options
            .read(true)
            .access_mode(DELETE_AND_SYNCHRONIZE)
            .share_mode(0x1 | 0x2 | 0x4);
        directory.open_with(name, &options)
    }

    /// Lays out the variable-length rename structure both APIs take.
    ///
    /// The two structures have the same shape; only the call differs.
    fn layout(root_directory: Option<&Dir>, to: &str) -> (Vec<u64>, usize, u32) {
        let mut name: Vec<u16> = Path::new(to).as_os_str().encode_wide().collect();
        let name_bytes = name.len() * std::mem::size_of::<u16>();
        name.push(0);

        let header = std::mem::size_of::<FILE_RENAME_INFO>();
        let words = (header + name_bytes).div_ceil(std::mem::size_of::<u64>());
        let mut buffer: Vec<u64> = vec![0; words];

        // SAFETY: the buffer is at least `header + name_bytes` bytes and aligned
        // to eight, which is what the structure needs for the handle it holds.
        unsafe {
            let info = buffer.as_mut_ptr().cast::<FILE_RENAME_INFO>();
            (*info).Anonymous.ReplaceIfExists = false;
            (*info).RootDirectory = root_directory.map_or(std::ptr::null_mut(), |directory| {
                directory.as_raw_handle() as HANDLE
            });
            (*info).FileNameLength = name_bytes as u32;
            std::ptr::copy_nonoverlapping(
                name.as_ptr(),
                std::ptr::addr_of_mut!((*info).FileName).cast::<u16>(),
                name.len(),
            );
        }
        (
            buffer,
            words * std::mem::size_of::<u64>(),
            name_bytes as u32,
        )
    }

    fn set_file_information(
        directory: &Dir,
        from: &str,
        root_directory: Option<&Dir>,
        to: &str,
    ) -> Result<(), String> {
        let file = open_source(directory, from).map_err(|error| format!("open: {error}"))?;
        let (buffer, size, _) = layout(root_directory, to);

        // SAFETY: the handle is borrowed for the call and the buffer is live and
        // of the length passed.
        let outcome = unsafe {
            SetFileInformationByHandle(
                file.as_raw_handle() as HANDLE,
                FileRenameInfo,
                buffer.as_ptr().cast(),
                size as u32,
            )
        };
        if outcome == 0 {
            let error = std::io::Error::last_os_error();
            return Err(format!("{error} (code {:?})", error.raw_os_error()));
        }
        Ok(())
    }

    fn nt_set_information(
        directory: &Dir,
        from: &str,
        root_directory: &Dir,
        to: &str,
    ) -> Result<(), String> {
        let file = open_source(directory, from).map_err(|error| format!("open: {error}"))?;
        let (buffer, size, _) = layout(Some(root_directory), to);

        // SAFETY: the same layout, which FILE_RENAME_INFORMATION shares; the
        // status block is a live, zeroed value of the right type.
        let mut status: IO_STATUS_BLOCK = unsafe { std::mem::zeroed() };
        let outcome = unsafe {
            NtSetInformationFile(
                file.as_raw_handle() as HANDLE,
                &raw mut status,
                buffer.as_ptr().cast(),
                size as u32,
                FILE_RENAME_INFORMATION_CLASS,
            )
        };
        if outcome < 0 {
            return Err(format!("NTSTATUS {outcome:#010x}"));
        }
        let _ = std::mem::size_of::<FILE_RENAME_INFORMATION>();
        Ok(())
    }

    fn report(what: &str, outcome: Result<(), String>) {
        match outcome {
            Ok(()) => println!("ok       {what}"),
            Err(why) => println!("FAILED   {what}\n         {why}"),
        }
    }
}
