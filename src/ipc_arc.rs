use std::alloc::Layout;
use std::sync::atomic::{AtomicUsize, Ordering};

use nix::errno::Errno;
use nix::libc::pthread_mutex_t;
use nix::{
    Error,
    libc::{self},
};

use crate::mem_handler::{MemHandler, SharedMemHandler, SharedMemResult};
use crate::{MutexGuard, RawMutex};

#[repr(C)]
struct InnerIpcArc<T> {
    mutex: RawMutex,
    counter: AtomicUsize,
    ptr: T,
}
impl<T> InnerIpcArc<T> {
    fn inc_counter(&mut self, val: usize) {
        self.counter
            .fetch_add(val, std::sync::atomic::Ordering::SeqCst);
    }

    fn dec_counter(&mut self, val: usize) -> usize {
        // if self.counter.load(Ordering::SeqCst) == 0 {
        //     println!("Overflow {}, {}", self.counter., val);
        // }
        self.counter.fetch_sub(val, Ordering::SeqCst)
    }
}
#[repr(C)]
pub struct IpcArc<T> {
    name: String,
    inner: *mut InnerIpcArc<T>,

    mem_handler: MemHandler,
    shared_handler: SharedMemHandler,
}
impl<T> IpcArc<T> {
    pub fn lock(&self) -> MutexGuard<'_, T> {
        MutexGuard {
            mutex: &self.inner().mutex,
            inner: &mut self.inner().ptr,
        }
    }

    pub fn as_ref(&self) -> &T {
        unsafe { &self.inner.as_ref().unwrap().ptr }
    }

    pub fn as_mut(&self) -> &mut T {
        unsafe { &mut self.inner.as_mut().unwrap().ptr }
    }
    fn inner(&self) -> &mut InnerIpcArc<T> {
        unsafe { self.inner.as_mut().unwrap() }
    }

    pub fn open(name: &str) -> Result<Self, Error> {
        let size = Layout::new::<(AtomicUsize, libc::pthread_mutex_t, T)>().size();
        let shared_memory_handler = SharedMemHandler::open(name)?;

        // map the fd to a page
        let mem_handler = MemHandler::map(shared_memory_handler.fd_ref(), size)?;

        // self.inner = self.mem_handler.as_ptr() as *mut InnerIpcArc<T>;

        let ipc = Self {
            name: name.to_owned(),
            shared_handler: shared_memory_handler,
            inner: mem_handler.as_ptr() as *mut InnerIpcArc<T>,
            mem_handler: mem_handler,
        };
        ipc.inc_counter();
        Ok(ipc)
    }
    /// create shared memory reg, if it already exist open it directly
    pub fn create_or_open(name: &str, val: T) -> Result<Self, Error> {
        // built-in to calculate allginment
        let size = Layout::new::<(AtomicUsize, libc::pthread_mutex_t, T)>().size();
        let mut is_owner = false;
        let shared_mem = match SharedMemHandler::try_open(name, size as i64) {
            SharedMemResult::Opened(handler) => {
                is_owner = true;
                handler
            }
            SharedMemResult::AlreadyExist => SharedMemHandler::open(name)?,
            SharedMemResult::Error(e) => return Err(e),
        };

        // resize the shared object
        let mem_handler = MemHandler::map(shared_mem.fd_ref(), size)?;

        let ipc = Self {
            name: name.to_owned(),
            shared_handler: shared_mem,
            inner: mem_handler.as_ptr() as *mut InnerIpcArc<T>,
            mem_handler: mem_handler,
        };
        // Ok(ipc)

        if is_owner {
            // only the initial owner zeros the mem
            unsafe { std::ptr::write_bytes(&ipc.inner().mutex as *const _ as *mut u8, 0, size) };
            let mutex_ptr = &ipc.inner().mutex as *const _ as *mut pthread_mutex_t;
            RawMutex::init_mutex(mutex_ptr);
        }

        ipc.inc_counter();
        *ipc.as_mut() = val;

        Ok(ipc)
    }

    pub fn read_counter(&self) -> usize {
        // println!("{:p}", self.inner());
        self.inner().counter.load(Ordering::SeqCst)
    }

    pub fn inc_counter(&self) {
        self.inner().inc_counter(1);
    }

    pub fn dec_counter(&self) {
        self.inner().dec_counter(1);
    }

    pub fn unlink(&self) -> Result<(), Errno> {
        self.dec_counter();
        if self.read_counter() == 0 {
            self.force_unlink()?;
        }
        Ok(())
    }
    pub fn force_unlink(&self) -> Result<(), Errno> {
        self.shared_handler.unlink_shm()?;
        self.mem_handler.unmap()?;
        self.shared_handler.close()?;
        // shm_unlink(self.name.as_str())?;
        // unsafe {
        //     munmap(self.mapped_mem.unwrap(), self.mapped_mem_size);
        // }
        // unsafe { close(self.mem_fd.unwrap()) };
        Ok(())
    }
}
