use std::path::Path;

pub fn publish_no_replace(staging: &Path, output: &Path) -> std::io::Result<()> {
    std::fs::hard_link(staging, output)
}

pub fn close_file(file: std::fs::File) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::fd::IntoRawFd;
        let descriptor = file.into_raw_fd();
        if unsafe { libc::close(descriptor) } != 0 {
            return Err(std::io::Error::last_os_error());
        }
        Ok(())
    }
    #[cfg(windows)]
    {
        use std::os::windows::io::IntoRawHandle;
        let handle = file.into_raw_handle();
        if unsafe { windows::CloseHandle(handle) } == 0 {
            return Err(std::io::Error::last_os_error());
        }
        Ok(())
    }
    #[cfg(not(any(unix, windows)))]
    {
        drop(file);
        Err(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "checked file closure requires Unix or Windows",
        ))
    }
}

pub fn identity(file: &std::fs::File) -> std::io::Result<(u64, u128)> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let metadata = file.metadata()?;
        Ok((metadata.dev(), u128::from(metadata.ino())))
    }
    #[cfg(windows)]
    {
        use std::os::windows::io::AsRawHandle;
        let mut identity = std::mem::MaybeUninit::<windows::FileIdInfo>::uninit();
        let result = unsafe {
            windows::GetFileInformationByHandleEx(
                file.as_raw_handle(),
                18,
                identity.as_mut_ptr().cast(),
                std::mem::size_of::<windows::FileIdInfo>() as u32,
            )
        };
        if result == 0 {
            return Err(std::io::Error::last_os_error());
        }
        let identity = unsafe { identity.assume_init() };
        Ok((
            identity.volume_serial_number,
            u128::from_le_bytes(identity.file_id),
        ))
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = file;
        Err(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "file identity requires Unix or Windows",
        ))
    }
}

#[cfg(windows)]
mod windows {
    #[repr(C)]
    pub struct FileIdInfo {
        pub volume_serial_number: u64,
        pub file_id: [u8; 16],
    }

    #[link(name = "kernel32")]
    extern "system" {
        pub fn GetFileInformationByHandleEx(
            handle: *mut std::ffi::c_void,
            class: i32,
            information: *mut std::ffi::c_void,
            size: u32,
        ) -> i32;
        pub fn CloseHandle(handle: *mut std::ffi::c_void) -> i32;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn descriptor_identity_detects_same_size_replacement() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("artifact");
        std::fs::write(&path, b"first").unwrap();
        let original = std::fs::File::open(&path).unwrap();
        let expected = identity(&original).unwrap();
        std::fs::rename(&path, root.path().join("original")).unwrap();
        std::fs::write(&path, b"other").unwrap();
        let replacement = std::fs::File::open(&path).unwrap();
        assert_ne!(identity(&replacement).unwrap(), expected);
        assert_eq!(identity(&original).unwrap(), expected);
        close_file(replacement).unwrap();
        close_file(original).unwrap();
    }
}
