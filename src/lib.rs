use std::num::NonZero;
use std::ops::Add;
use std::ptr;

use nix::sys::mman::{MapFlags, ProtFlags, mmap};
use nix::{
    Error,
    fcntl::OFlag,
    libc::{self},
    sys::{mman::shm_open, stat::Mode},
    unistd::ftruncate,
};
pub struct IpcArc<T> {
    counter: *mut u32,
    ptr: *mut T,
}
impl<T> IpcArc<T> {
    pub fn new() -> Self {
        Self {
            counter: std::ptr::null_mut(),
            ptr: std::ptr::null_mut(),
        }
    }

    pub fn open(&mut self, name: &str, val: T) -> Result<(), Error> {
        let fd = shm_open(
            name,
            OFlag::O_CREAT | OFlag::O_RDWR,
            Mode::from_bits_truncate(libc::S_IRUSR | libc::S_IWUSR),
        )
        .expect("shm_open failed");
        let size = 64;

        ftruncate(&fd, size).expect("ftruncate failed");
        let addr = unsafe {
            mmap(
                None,
                NonZero::new(20).unwrap(),
                ProtFlags::PROT_READ | ProtFlags::PROT_WRITE,
                MapFlags::MAP_SHARED,
                &fd,
                0,
            )?
        };

        let nptr = addr.as_ptr() as *mut T;

        // mem layout off: 0
        self.counter = nptr as *mut u32;
        let counter_ptr = self.counter;
        if self.read_counter() == 999 {
            unsafe {
                *counter_ptr = 0;
            }
        } else {
            unsafe {
                *counter_ptr = *counter_ptr + 1;
            }
        }
        println!("count: {}", self.read_counter());
        // offset 0 + size(u32)
        self.ptr = unsafe { nptr.add(size_of::<u32>()) };

        Ok(())
    }
    fn read_counter(&self) -> u32 {
        unsafe { ptr::read(self.counter as *mut u32) }
    }
}

#[cfg(test)]
mod tests {

    use std::{ptr, thread, time::Duration};

    use nix::libc::fork;

    use super::*;
    #[test]
    fn test() {
        let mut ins: IpcArc<u64> = IpcArc::new();

        ins.open("/hello", 43).unwrap();

        unsafe {
            ptr::write(ins.ptr, 43);
        }

        let reference: &mut u64 = unsafe { &mut *ins.ptr };
        println!("re:{}", *reference);

        let s = unsafe { fork() };
        if s == 0 {
            thread::sleep(Duration::from_secs(1));

            unsafe {
                ptr::write(ins.ptr, 1111);
            }
            println!("hehe: {s}");
            ins.open("/hello", 43).unwrap();

            let reference: &mut u64 = unsafe { &mut *ins.ptr };
            println!("re:{}", *reference);
        } else {
            thread::sleep(Duration::from_secs(2));
            println!("else: {s}");
            ins.open("/hello", 43).unwrap();

            let reference: &mut u64 = unsafe { &mut *ins.ptr };

            unsafe {
                ptr::write(ins.ptr, 88);
            }
            println!("re:{}", *reference);
        }

        println!("re:{}", *reference);
        // unsafe {
        //     assert!(!ins.ptr.is_null(), "Pointer is null");
        //     assert_eq!(
        //         (ins.ptr as usize) % std::mem::align_of::<u64>(),
        //         0,
        //         "Pointer is misaligned"
        //     );
        //
        //     // Write to mmaped memory safely
        //     std::ptr::write(ins.ptr, 43u64);
        //
        //     // Read it back
        //     let reference: &mut u64 = &mut *ins.ptr;
        //     println!("re:{}", *reference);
        // }
    }
}
