#[cfg(feature = "std")]
pub(crate) use std::sync::OnceLock;

#[cfg(not(feature = "std"))]
mod no_std {
    use alloc::boxed::Box;
    use core::hint::spin_loop;
    use core::ptr;
    use core::sync::atomic::{AtomicPtr, AtomicU8, Ordering};

    const UNINIT: u8 = 0;
    const INITING: u8 = 1;
    const READY: u8 = 2;

    pub(crate) struct OnceLock<T> {
        state: AtomicU8,
        ptr: AtomicPtr<T>,
    }

    unsafe impl<T: Send + Sync> Sync for OnceLock<T> {}
    unsafe impl<T: Send> Send for OnceLock<T> {}

    impl<T> OnceLock<T> {
        pub(crate) const fn new() -> Self {
            Self {
                state: AtomicU8::new(UNINIT),
                ptr: AtomicPtr::new(ptr::null_mut()),
            }
        }

        pub(crate) fn get_or_init<F>(&self, init: F) -> &T
        where
            F: FnOnce() -> T,
        {
            if self.state.load(Ordering::Acquire) == READY {
                return self.get_ready();
            }

            if self
                .state
                .compare_exchange(UNINIT, INITING, Ordering::AcqRel, Ordering::Acquire)
                .is_ok()
            {
                let raw = Box::into_raw(Box::new(init()));
                self.ptr.store(raw, Ordering::Release);
                self.state.store(READY, Ordering::Release);
                return self.get_ready();
            }

            while self.state.load(Ordering::Acquire) != READY {
                spin_loop();
            }
            self.get_ready()
        }

        pub(crate) fn get(&self) -> Option<&T> {
            if self.state.load(Ordering::Acquire) == READY {
                Some(self.get_ready())
            } else {
                None
            }
        }

        pub(crate) fn set(&self, value: T) -> Result<(), T> {
            let mut value = Some(value);
            self.get_or_init(|| value.take().expect("value present"));
            match value {
                None => Ok(()),
                Some(v) => Err(v),
            }
        }

        fn get_ready(&self) -> &T {
            let ptr = self.ptr.load(Ordering::Acquire);
            debug_assert!(!ptr.is_null());
            unsafe { &*ptr }
        }
    }

    // The value lives in a Box: without this every picture's motion data (about 1 MB at 720p) leaked.
    impl<T> Drop for OnceLock<T> {
        fn drop(&mut self) {
            let ptr = *self.ptr.get_mut();
            if !ptr.is_null() {
                // SAFETY: `ptr` came from `Box::into_raw` in `get_or_init` and `&mut self` means no reader is left.
                drop(unsafe { Box::from_raw(ptr) });
            }
        }
    }

    #[cfg(test)]
    mod tests {
        use super::OnceLock;
        use alloc::rc::Rc;

        #[test]
        fn the_value_is_dropped() {
            let v = Rc::new(());
            let once = OnceLock::new();
            once.get_or_init(|| v.clone());
            assert_eq!(Rc::strong_count(&v), 2);
            drop(once);
            assert_eq!(Rc::strong_count(&v), 1);
        }
    }
}

#[cfg(not(feature = "std"))]
pub(crate) use no_std::OnceLock;
