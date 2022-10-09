use std::os::unix::io::RawFd;
use std::path::Path;
use std::ptr::null_mut;

use nix::errno::Errno;
use nix::fcntl::OFlag;
use nix::sys::mman::{mmap, munmap, shm_open, shm_unlink, MapFlags, ProtFlags};
use nix::sys::stat::Mode;
use nix::unistd::{close, ftruncate};

use libc::{c_void, off_t};

///
/// ShmDefinition describes a shared memory object through its path and its allocated size.
///
#[derive(Debug)]
pub struct ShmDefinition {
    /// The path at which the shared memory file descriptor will be open
    /// (typially /dev/shm/..., /dev/hugepages/...)
    pub path: String,
    /// The size of the memory to allocate for this shared memory block.
    pub size: usize,
}

///
/// Codes used to report errors when using Shared Memory on Posix systems.
///
/// These codes are mapped from the libc reported error codes. The same integer code
/// may be mapped to multiple ErrorCode values to reflect the context of the error.
///
#[derive(Debug, PartialEq)]
pub enum ErrorCode {
    /// The process could not access the given path to open a shared memory object.
    ShmPathAccessDenied,
    /// The process tried to create a shared memory object but the path already existed.
    ShmPathAlreadyExists,
    /// The path given to open the shared memory object is invalid.
    ShmPathInvalid,
    /// The maximum number of open file descriptors for this process was exceeded.
    ProcessTooManyOpenFD,
    /// The path for the shared memory object is too long.
    ShmPathTooLong,
    /// The maximum number of open files for the system was exceeded.
    SystemTooManyOpenFiles,
    /// The path where the file descriptor should be open does not exist.
    ShmPathDoesNotExist,
    /// A signal interrupted the truncation.
    TruncateInterrupted,
    /// The specified size was too small or too large.
    InvalidTruncationSize,
    /// The arguments to mmap were invalid.
    InvalidMMapArguments,
    /// Not enough memory to mmap
    OutOfMemory,
    /// Incorrect or insufficient permission to apply mmmap
    MissingPermission,
    /// An IO error occured when closing the shared memory object file descriptor.
    CloseIOError,
    /// Close of the shared memory object file descriptor was interrupted by a signal.
    CloseInterrupted,
    /// Attempt to unlink a file that does not exist.
    UnlinkingANonExistentFile,
    /// An unmapped error was reported with the given return code.
    Unknown(Errno),
}

impl ShmDefinition {
    ///
    /// Create a shared memory object from this definition.
    /// The mapped object is owned and will be unlinked when the OwnerShmMap is dropped.
    /// ```
    /// use rshm::shm::ShmDefinition;
    ///
    /// let definition = ShmDefinition {
    ///     path: "test1".to_string(),
    ///     size: 1024,
    /// };
    /// let _shm = definition.create().unwrap();
    /// let metadata = std::fs::metadata("/dev/shm/test1").unwrap();
    /// assert!(metadata.is_file());
    /// assert_eq!(1024, metadata.len());
    /// ```
    ///
    pub fn create(self) -> Result<OwnedShmMap, ErrorCode> {
        let path = self.path.as_str();
        shm_open(
            path,
            OFlag::O_CREAT | OFlag::O_EXCL | OFlag::O_RDWR, //create exclusively (error if collision) and write to allow resize
            Mode::S_IRUSR | Mode::S_IWUSR,                  //Permission allow user+rw
        )
        .map_err(map_open_error)
        .and_then(|fd| {
            (&self)
                .create_mmap(fd, ProtFlags::PROT_READ | ProtFlags::PROT_WRITE)
                .and_then(|p| {
                    Ok(OwnedShmMap {
                        definition: self,
                        head: p as *const u8,
                    })
                })
        })
    }

    ///
    /// opens an existing shared memory object based on this definition.
    /// The mapped object is not considered owner and will not be unlinked when the ShmMap is dropped.
    ///
    /// ```
    /// use rshm::shm::ShmDefinition;
    ///
    /// let definition_owned = ShmDefinition {
    ///     path: "example".to_string(),
    ///     size: 1024,
    /// };
    /// let definition = ShmDefinition {
    ///     path: "example".to_string(),
    ///     size: 1024,
    /// };
    /// let owned_shm = definition_owned.create().unwrap();
    /// let shm = definition.open().unwrap();
    /// unsafe { (owned_shm.head() as *mut u8).write(8) };
    /// assert_eq!(8, unsafe { (shm.head() as *const u8).read() });
    /// ```
    ///
    pub fn open(self) -> Result<ShmMap, ErrorCode> {
        shm_open(
            self.path.as_str(),
            OFlag::O_RDWR,                 // write to allow resize
            Mode::S_IRUSR | Mode::S_IWUSR, //Permission allow user+rw
        )
        .map_err(map_open_error)
        .and_then(|fd| {
            (&self)
                .create_mmap(fd, ProtFlags::PROT_READ | ProtFlags::PROT_WRITE)
                .and_then(|p| {
                    Ok(ShmMap {
                        definition: self,
                        head: p as *const u8,
                    })
                })
        })
    }

    fn create_mmap(&self, fd: RawFd, flags: ProtFlags) -> Result<*mut c_void, ErrorCode> {
        ftruncate(fd, self.size as off_t)
            .map_err(map_truncate_error)
            .and_then(|_| unsafe {
                mmap(
                    null_mut(),           // Desired addr
                    self.size,            // size of mapping
                    flags,                // Permissions on pages
                    MapFlags::MAP_SHARED, // What kind of mapping
                    fd,                   // fd
                    0,                    // Offset into fd
                )
                .map_err(map_mmap_error)
            })
            .and_then(|p| close(fd).map_err(map_close_error).and_then(|_| Ok(p)))
            .or_else(|err| {
                let _close_result = close(fd);
                let _removal_result =
                    std::fs::remove_file(Path::new(format!("/dev/shm/{}", self.path).as_str()));
                Err(err)
            })
    }
}

///
/// A mapped shared memory object that was created by some other process.
/// It will not be unlinked when dropped.
///
#[derive(Debug)]
pub struct ShmMap {
    /// Definition of the shared memory object that is mapped
    pub definition: ShmDefinition,
    /// The pointer to the start of the memory mapped object
    head: *const u8,
}

///
/// A mapped shared memory object that was created by this process.
/// It will be unlinked when dropped.
///
#[derive(Debug)]
pub struct OwnedShmMap {
    /// Definition of the shared memory object that is mapped
    pub definition: ShmDefinition,
    /// The pointer to the start of the memory mapped object
    head: *const u8,
}

impl Drop for OwnedShmMap {
    fn drop(&mut self) {
        unsafe { munmap(self.head as *mut _, self.definition.size) }
            .map_err(map_munmap_error)
            .and_then(|_| shm_unlink(self.definition.path.as_str()).map_err(map_unlink_error))
            .unwrap();
    }
}

impl OwnedShmMap {
    /// returns a pointer to the start of the mapped memory object
    pub fn head(&self) -> *const u8 {
        self.head
    }
}

impl Drop for ShmMap {
    fn drop(&mut self) {
        unsafe {
            munmap(self.head as *mut _, self.definition.size)
                .map_err(map_munmap_error)
                .unwrap()
        }
    }
}

impl ShmMap {
    /// returns a pointer to the start of the mapped memory object
    pub fn head(&self) -> *const u8 {
        self.head
    }
}

fn map_unlink_error(errno: Errno) -> ErrorCode {
    match errno {
        Errno::ENOENT => ErrorCode::UnlinkingANonExistentFile,
        other => ErrorCode::Unknown(other),
    }
}

fn map_munmap_error(errno: Errno) -> ErrorCode {
    match errno {
        other => ErrorCode::Unknown(other),
    }
}

fn map_close_error(errno: Errno) -> ErrorCode {
    match errno {
        Errno::EINTR => ErrorCode::CloseInterrupted,
        Errno::EIO => ErrorCode::CloseIOError,
        other => ErrorCode::Unknown(other),
    }
}

fn map_mmap_error(errno: Errno) -> ErrorCode {
    match errno {
        Errno::EINVAL => ErrorCode::InvalidMMapArguments,
        Errno::ENOMEM => ErrorCode::OutOfMemory,
        Errno::EPERM => ErrorCode::MissingPermission,
        other => ErrorCode::Unknown(other),
    }
}

fn map_truncate_error(errno: Errno) -> ErrorCode {
    match errno {
        Errno::EINTR => ErrorCode::TruncateInterrupted,
        Errno::EINVAL => ErrorCode::InvalidTruncationSize,
        Errno::E2BIG => ErrorCode::InvalidTruncationSize,
        other => ErrorCode::Unknown(other),
    }
}

fn map_open_error(errno: Errno) -> ErrorCode {
    match errno {
        Errno::EACCES => ErrorCode::ShmPathAccessDenied,
        Errno::EEXIST => ErrorCode::ShmPathAlreadyExists,
        Errno::EINVAL => ErrorCode::ShmPathInvalid,
        Errno::EMFILE => ErrorCode::ProcessTooManyOpenFD,
        Errno::ENAMETOOLONG => ErrorCode::ShmPathTooLong,
        Errno::ENFILE => ErrorCode::SystemTooManyOpenFiles,
        Errno::ENOENT => ErrorCode::ShmPathDoesNotExist,
        other => ErrorCode::Unknown(other),
    }
}

#[cfg(test)]
mod tests {
    use std::io::ErrorKind;

    use crate::shm::ErrorCode;

    use super::ShmDefinition;

    #[test]
    fn create_a_shared_memory_object_with_the_correct_size() {
        let definition = ShmDefinition {
            path: "test1".to_string(),
            size: 1024,
        };
        let _shm = definition.create().unwrap();
        let metadata = std::fs::metadata("/dev/shm/test1").unwrap();

        assert!(metadata.is_file());
        assert_eq!(1024, metadata.len());
    }

    #[test]
    fn drop_owned_shm_removes_the_shared_memory_object() {
        let definition = ShmDefinition {
            path: "test2".to_string(),
            size: 1024,
        };
        let shm = definition.create().unwrap();
        drop(shm);

        let metadata_result = std::fs::metadata("/dev/shm/test2");
        let err = metadata_result.expect_err("File not removed.");
        assert_eq!(ErrorKind::NotFound, err.kind());
    }

    #[test]
    fn open_maps_an_existing_shared_memory_object() {
        let definition_owned = ShmDefinition {
            path: "test3".to_string(),
            size: 1024,
        };
        let definition = ShmDefinition {
            path: "test3".to_string(),
            size: 1024,
        };
        let owned_shm = definition_owned.create().unwrap();
        let shm = definition.open().unwrap();

        unsafe { (owned_shm.head() as *mut u8).write(8) };
        assert_eq!(8, unsafe { (shm.head() as *const u8).read() });
        unsafe { (owned_shm.head() as *mut u8).write(0) };
        assert_eq!(0, unsafe { (shm.head() as *const u8).read() });
    }

    #[test]
    fn drop_shm_does_not_remove_the_shared_memory_object() {
        let definition_owned = ShmDefinition {
            path: "test4".to_string(),
            size: 1024,
        };
        let definition = ShmDefinition {
            path: "test4".to_string(),
            size: 1024,
        };
        let _owned_shm = definition_owned.create().unwrap();
        let shm = definition.open().unwrap();

        drop(shm);

        let metadata = std::fs::metadata("/dev/shm/test4").unwrap();
        assert!(metadata.is_file());
    }

    #[test]
    fn create_reports_an_error_when_path_is_invalid() {
        let definition = ShmDefinition {
            path: "/dev/shm/test".to_string(),
            size: 1024,
        };
        let error = definition.create().unwrap_err();

        assert_eq!(ErrorCode::ShmPathInvalid, error);
    }

    #[test]
    fn create_reports_an_error_when_path_already_exists() {
        let definition1 = ShmDefinition {
            path: "test6".to_string(),
            size: 1024,
        };
        let definition2 = ShmDefinition {
            path: "test6".to_string(),
            size: 1024,
        };
        let _shm = definition1.create().unwrap();
        let error = definition2.create().unwrap_err();

        assert_eq!(ErrorCode::ShmPathAlreadyExists, error);
    }

    #[test]
    fn open_reports_an_error_when_path_does_not_exists() {
        let definition = ShmDefinition {
            path: "test7".to_string(),
            size: 1024,
        };
        let error = definition.open().unwrap_err();

        assert_eq!(ErrorCode::ShmPathDoesNotExist, error);
    }

    #[test]
    fn open_reports_an_error_when_size_is_invalid() {
        let definition = ShmDefinition {
            path: "test8".to_string(),
            size: 0,
        };
        let error = definition.create().unwrap_err();

        assert_eq!(ErrorCode::InvalidMMapArguments, error);
    }
}
