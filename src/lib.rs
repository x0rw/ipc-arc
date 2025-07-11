use std::alloc::Layout;
use std::mem::MaybeUninit;
use std::num::NonZero;
use std::os::fd::{IntoRawFd, RawFd};
use std::ptr;
use std::slice::from_raw_parts;

use nix::errno::Errno;
use nix::libc::{
    PTHREAD_PROCESS_SHARED, close, pthread_mutex_init, pthread_mutex_lock, pthread_mutex_t,
    pthread_mutex_unlock, pthread_mutexattr_destroy, pthread_mutexattr_init,
    pthread_mutexattr_setpshared, pthread_mutexattr_t,
};
use nix::sys::mman::{MapFlags, ProtFlags, mmap, shm_unlink};
use nix::{
    Error,
    fcntl::OFlag,
    libc::{self},
    sys::{mman::shm_open, stat::Mode},
    unistd::ftruncate,
};

#[repr(C)]
struct InnerIpcArc<T> {
    mutex: pthread_mutex_t,
    counter: u32,
    ptr: T,
}
impl<T> InnerIpcArc<T> {
    fn as_ref(&self) -> &T {
        &self.ptr
    }
    pub fn counter(&self) -> u32 {
        self.counter
    }

    fn set_counter(&mut self, val: u32) {
        self.counter = val;
    }

    fn inc_counter(&mut self, val: u32) {
        self.counter = self.counter + val;
    }
    fn as_slice(&self) -> &[u8] {
        let slice = unsafe { from_raw_parts(&self.ptr as *const _ as *mut u8, size_of::<T>()) };

        slice
    }
}
#[repr(C)]
pub struct IpcArc<T> {
    inner: *mut InnerIpcArc<T>,
    name: String,
    mem_fd: Option<RawFd>,
}
impl<T> IpcArc<T> {
    pub fn new() -> Self {
        Self {
            inner: std::ptr::null_mut(),
            name: String::new(),
            mem_fd: None,
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

    pub fn open(&mut self, name: &str, val: T) -> Result<(), Error> {
        self.name = name.to_owned();

        // built-in to calculate allginment
        let size = Layout::new::<(u32, libc::pthread_mutex_t, T)>().size();

        let (fd, is_owner) = match shm_open(
            name,
            OFlag::O_CREAT | OFlag::O_RDWR | OFlag::O_EXCL,
            Mode::from_bits_truncate(libc::S_IRUSR | libc::S_IWUSR),
        ) {
            Ok(fd) => {
                ftruncate(&fd, size as i64).expect("ftruncate failed");
                (fd, true)
            }
            Err(nix::Error::EEXIST) => {
                let fd = shm_open(name, OFlag::O_RDWR, Mode::empty())?;
                (fd, false)
            }

            Err(e) => return Err(e),
        };

        // resize the shared object
        let addr = unsafe {
            mmap(
                None,
                NonZero::new(size).unwrap(),
                ProtFlags::PROT_READ | ProtFlags::PROT_WRITE,
                MapFlags::MAP_SHARED,
                &fd,
                0,
            )?
        };

        let raw_fd = fd.into_raw_fd();
        self.mem_fd = Some(raw_fd);

        self.inner = addr.as_ptr() as *mut InnerIpcArc<T>;

        let mutex_ptr = &self.inner().mutex as *const _ as *mut pthread_mutex_t;
        // let base = addr.as_ptr() as *mut u8;
        //
        // let mutex_ptr = base as *mut pthread_mutex_t;
        // let after_mutex = unsafe { base.add(size_of::<pthread_mutex_t>()) };
        // let counter_offset = after_mutex.align_offset(align_of::<u32>());
        // let counter_ptr = unsafe { after_mutex.add(counter_offset) };
        // let after_counter = unsafe { counter_ptr.add(size_of::<u32>()) };
        //
        // let type_offset = after_counter.align_offset(align_of::<T>());
        // let value_ptr = unsafe { after_counter.add(type_offset) };

        // self.mutex = mutex_ptr;
        // self.counter = counter_ptr as *mut u32;
        // self.ptr = value_ptr as *mut T;

        if is_owner {
            unsafe { std::ptr::write_bytes(self.inner, 0, size) };
            println!("============= owner here ==================");
            //----------------------- ATTR ----------------------------------//
            // set/init mutex attr
            let mut attr = MaybeUninit::<pthread_mutexattr_t>::uninit();
            let ret = unsafe { pthread_mutexattr_init(attr.as_mut_ptr()) };
            if ret != 0 {
                panic!("pthread_mutexattr_init failed with {ret}");
            }

            let mut attr = unsafe { attr.assume_init() };

            // set the mutex as shared
            let ret = unsafe {
                pthread_mutexattr_setpshared(&attr as *const _ as *mut _, PTHREAD_PROCESS_SHARED)
            };
            assert_eq!(ret, 0, "pthread_mutex_init failed");

            let ret = unsafe {
                pthread_mutex_init(
                    &self.inner().mutex as *const _ as *mut _,
                    &attr as *const _ as *mut _,
                )
            };

            unsafe { pthread_mutexattr_destroy(&mut attr) };
            assert_eq!(ret, 0, "pthread_mutex_init failed");
        }
        // lock
        let ret = unsafe { pthread_mutex_lock(mutex_ptr) };
        assert_eq!(ret, 0, "pthread_mutex_lock failed");

        println!("===================================");
        println!("d: {:?}", unsafe { (*self.inner).as_slice() });
        println!("counter: {:?}", unsafe { (*self.inner).counter() });
        println!("counter: {:?}", self.read_counter());
        println!("===================================");

        self.inner().inc_counter(1);

        let ret = unsafe { pthread_mutex_unlock(mutex_ptr) };
        assert_eq!(ret, 0, "pthread_mutex_unlock failed");

        *self.as_mut() = val;
        // self.ptr.write(val);

        Ok(())
    }

    fn read_counter(&self) -> u32 {
        self.inner().counter
    }

    pub fn inc_counter(&self) {
        let mutex_ptr = &self.inner().mutex as *const _ as *mut pthread_mutex_t;
        let ret = unsafe { pthread_mutex_lock(mutex_ptr) };
        assert_eq!(ret, 0, "pthread_mutex_lock failed");

        self.inner().inc_counter(1);
        let ret = unsafe { pthread_mutex_unlock(mutex_ptr) };
        assert_eq!(ret, 0, "pthread_mutex_unlock failed");
    }

    pub fn unlink(&self) -> Result<(), Errno> {
        self.inner().set_counter(self.read_counter() - 1);
        if self.read_counter() == 1 {
            shm_unlink(self.name.as_str())?;
        }
        Ok(())
    }
    pub fn force_unlink(&self) -> Result<(), Errno> {
        shm_unlink(self.name.as_str())?;
        // unsafe {
        //     munmap(addr.as_ptr(), size);
        // }
        unsafe { close(self.mem_fd.unwrap()) };
        Ok(())
    }
}

#[cfg(test)]
mod tests {

    #[test]
    fn test_ipcarc_multiprocess_counter() {
        use super::*;

        use nix::unistd::{ForkResult, fork};
        use std::process::exit;

        const NUM_PROCESSES: usize = 20;
        const INCREMENTS_PER_PROCESS: usize = 10;

        // Setup in parent
        let mut shared = IpcArc::<u64>::new();

        // shared.force_unlink().unwrap();
        shared.open("/hello", 42).unwrap();

        let mut children = vec![];

        for _ in 0..NUM_PROCESSES {
            match unsafe { fork() } {
                Ok(ForkResult::Child) => {
                    let mut child = IpcArc::<u64>::new();
                    child.open("/hello", 34).unwrap();

                    for _ in 0..INCREMENTS_PER_PROCESS {
                        child.inc_counter();
                    }

                    // println!(
                    //     "[child {}] counter: {}",
                    //     std::process::id(),
                    //     child.read_counter()
                    // );

                    // Do NOT unlink in child
                    exit(0);
                }
                Ok(ForkResult::Parent { child, .. }) => {
                    children.push(child);
                }
                Err(e) => panic!("fork failed: {e}"),
            }
        }

        // Wait for children
        for pid in children {
            nix::sys::wait::waitpid(pid, None).expect("waitpid failed");
        }

        let final_counter = shared.read_counter();
        let expected = (NUM_PROCESSES * INCREMENTS_PER_PROCESS + 20 + 1) as u32;

        println!("[parent] final counter: {}", final_counter);

        shared.force_unlink().unwrap();
        assert_eq!(
            final_counter, expected,
            "Expected {}, got {}",
            expected, final_counter
        );
    }
}
