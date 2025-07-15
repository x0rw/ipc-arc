use std::{
    ffi::c_void,
    num::NonZero,
    os::fd::{AsRawFd, BorrowedFd, IntoRawFd, RawFd},
};

use nix::{
    Error,
    errno::Errno,
    fcntl::OFlag,
    libc::{self, munmap},
    sys::{
        mman::{MapFlags, ProtFlags, mmap, shm_open, shm_unlink},
        stat::Mode,
    },
    unistd::{close, ftruncate},
};

pub(crate) struct MemHandler {
    mapped_mem: Option<*mut c_void>,
    mapped_mem_size: usize,
}

pub(crate) struct SharedMemHandler {
    fd: RawFd,
    name: String,
}

/// Shared Memory handler result on creation
pub enum SharedMemResult {
    Opened(SharedMemHandler),
    AlreadyExist,
    Error(Errno),
}

/// shared memory handler
impl SharedMemHandler {
    /// try opening a memory region based on its name
    /// if it exist return AlreadyExist, otherwise truncate it to the
    /// appropriate size and return the handler
    pub(crate) fn try_open(name: &str, size: i64) -> SharedMemResult {
        match shm_open(
            name,
            OFlag::O_CREAT | OFlag::O_RDWR | OFlag::O_EXCL,
            Mode::from_bits_truncate(libc::S_IRUSR | libc::S_IWUSR),
        ) {
            Ok(fd) => {
                ftruncate(&fd, size as i64).expect("ftruncate failed");
                return SharedMemResult::Opened(Self {
                    fd: fd.into_raw_fd(),
                    name: name.to_owned(),
                });
            }
            Err(nix::Error::EEXIST) => {
                return SharedMemResult::AlreadyExist;
                // let fd = shm_open(name, OFlag::O_RDWR, Mode::empty())?;
                // (fd, false)
            }

            Err(e) => SharedMemResult::Error(e),
        }
    }

    pub(crate) fn open(name: &str) -> Result<Self, Error> {
        let fd = shm_open(name, OFlag::O_RDWR, Mode::empty())?;
        Ok(Self {
            fd: fd.into_raw_fd(),
            name: name.to_owned(),
        })
    }

    /// create a shared memory region specifying a name and the size of the region
    pub(crate) fn create(name: &str, size: i64) -> Result<SharedMemHandler, Error> {
        let fd = shm_open(
            name,
            OFlag::O_CREAT | OFlag::O_RDWR,
            Mode::from_bits_truncate(libc::S_IRUSR | libc::S_IWUSR),
        )?;
        ftruncate(&fd, size as i64).expect("ftruncate failed");
        Ok(Self {
            fd: fd.into_raw_fd(),
            name: name.to_owned(),
        })
    }
    pub(crate) fn fd_ref(&self) -> RawFd {
        self.fd
    }

    /// unlink the shared memory
    /// Safety: before closing this, check if other processes still hold a  
    /// reference to the shared memory region
    pub(crate) fn unlink_shm(&self) -> Result<(), Error> {
        shm_unlink(self.name.as_str())?;
        Ok(())
    }

    /// close the file discriptor associated to this Handler
    pub(crate) fn close(&self) -> Result<(), Error> {
        close(self.fd.as_raw_fd())?;
        Ok(())
    }
}
impl MemHandler {
    pub(crate) fn map(fd: RawFd, size: usize) -> Result<Self, Errno> {
        let addr = unsafe {
            mmap(
                None,
                NonZero::new(size).unwrap(),
                ProtFlags::PROT_READ | ProtFlags::PROT_WRITE,
                MapFlags::MAP_SHARED,
                BorrowedFd::borrow_raw(fd),
                0,
            )?
        };
        Ok(Self {
            mapped_mem: Some(addr.as_ptr()),
            mapped_mem_size: size,
        })
    }
    /// get a pointer to the mapped shared mem page
    pub(crate) fn as_ptr(&self) -> *mut c_void {
        self.mapped_mem.unwrap()
    }
    pub(crate) fn unmap(&self) -> Result<(), Error> {
        let ret = unsafe { munmap(self.mapped_mem.unwrap(), self.mapped_mem_size) };
        if ret != 0 {
            panic!("munmap failed");
        }
        Ok(())
    }
}
