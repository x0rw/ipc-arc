use std::{marker::PhantomData, sync::atomic::AtomicUsize};

struct IpcArc<T> {
    ptr: *mut T,
    mutex: IpcArcMutex,
    ref_count: *const AtomicUsize,
    _phantom: PhantomData<T>,
}
#[cfg(test)]
mod tests {
    use super::*;
}
