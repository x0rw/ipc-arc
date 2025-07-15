use std::{
    mem::MaybeUninit,
    ops::{Deref, DerefMut},
};

use nix::libc::{
    PTHREAD_PROCESS_SHARED, pthread_mutex_init, pthread_mutex_lock, pthread_mutex_t,
    pthread_mutex_unlock, pthread_mutexattr_destroy, pthread_mutexattr_init,
    pthread_mutexattr_setpshared, pthread_mutexattr_t,
};

#[repr(C)]
pub(crate) struct RawMutex {
    inner: pthread_mutex_t,
}
pub struct MutexGuard<'a, T> {
    pub mutex: &'a RawMutex,
    pub inner: &'a mut T,
}
impl<'a, T> Deref for MutexGuard<'_, T> {
    type Target = T;
    fn deref(&self) -> &Self::Target {
        self.inner
    }
}
impl<'a, T> DerefMut for MutexGuard<'_, T> {
    fn deref_mut(&mut self) -> &mut T {
        self.inner
    }
}
impl<'a, T> MutexGuard<'a, T> {}

impl<'a, T> Drop for MutexGuard<'_, T> {
    fn drop(&mut self) {
        self.mutex.unlock();
    }
}

impl RawMutex {
    pub(crate) fn init_mutex(mutex_addr: *mut pthread_mutex_t) -> RawMutex {
        let mut attr = MaybeUninit::<pthread_mutexattr_t>::uninit();

        // init mutex
        let ret = unsafe { pthread_mutexattr_init(attr.as_mut_ptr()) };
        if ret != 0 {
            panic!("pthread_mutexattr_init failed with {ret}");
        }

        let mut attr = unsafe { attr.assume_init() };

        // set the mutex as shared
        let ret = unsafe {
            pthread_mutexattr_setpshared(&attr as *const _ as *mut _, PTHREAD_PROCESS_SHARED)
        };
        assert_eq!(ret, 0, "pthread_mutexattr_setpshared failed");

        // bound it to mutex_addr (base pointer)
        let ret = unsafe { pthread_mutex_init(mutex_addr, &attr as *const _ as *mut _) };
        assert_eq!(ret, 0, "mutex init failed");

        // destroy the attributes
        unsafe { pthread_mutexattr_destroy(&mut attr) };
        assert_eq!(ret, 0, "destroying mutexattr failed");

        Self {
            inner: unsafe { *mutex_addr },
        }
    }
    fn inner_ptr(&self) -> *mut pthread_mutex_t {
        &self.inner as *const _ as *mut pthread_mutex_t
    }
    pub(crate) fn lock(&self) {
        let ret = unsafe { pthread_mutex_lock(self.inner_ptr()) };
        assert_eq!(ret, 0, "pthread_mutex_lock failed");
    }

    pub(crate) fn unlock(&self) {
        let ret = unsafe { pthread_mutex_unlock(self.inner_ptr()) };
        assert_eq!(ret, 0, "pthread_mutex_unlock failed");
    }
}
