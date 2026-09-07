// Copyright (c) Microsoft Corporation
// License: MIT OR Apache-2.0

#[cfg(test)]
mod tests {
    use std::sync::{Mutex, MutexGuard};

    use wdk::sync::{PushLock, RwLock, RwSpinLock};
    use wdk_sys::test_stubs::{
        PUSH_LOCK_EXCLUSIVE_EVENTS,
        PUSH_LOCK_SHARED_EVENTS,
        SPIN_LOCK_EXCLUSIVE_AT_DPC_LEVEL_EVENTS,
        SPIN_LOCK_EXCLUSIVE_EVENTS,
        SPIN_LOCK_SHARED_AT_DPC_LEVEL_EVENTS,
        SPIN_LOCK_SHARED_EVENTS,
        push_lock_events,
        reset_push_lock_events,
        reset_spin_lock_events,
        spin_lock_events,
    };

    static KERNEL_STUBS: Mutex<()> = Mutex::new(());

    fn lock_kernel_stubs() -> MutexGuard<'static, ()> {
        match KERNEL_STUBS.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        }
    }

    #[test]
    fn rw_lock_read_and_write_guards_access_value() {
        let _stub_guard = lock_kernel_stubs();
        let lock = RwLock::try_new(1_u32).expect("ERESOURCE initialization should succeed");

        assert_eq!(*lock.read(), 1);

        {
            let mut value = lock.write();
            *value = 2;
        }

        assert_eq!(
            *lock.try_read().expect("shared acquisition should succeed"),
            2
        );

        {
            let mut value = lock
                .try_write()
                .expect("exclusive acquisition should succeed");
            *value += 1;
        }

        assert_eq!(*lock.read(), 3);
    }

    #[test]
    fn rw_lock_get_mut_accesses_value_without_locking() {
        let _stub_guard = lock_kernel_stubs();
        let mut lock = RwLock::try_new(1_u32).expect("ERESOURCE initialization should succeed");

        *lock.get_mut() = 7;

        assert_eq!(*lock.read(), 7);
    }

    #[test]
    fn rw_lock_try_methods_reject_recursive_acquisition() {
        let _stub_guard = lock_kernel_stubs();
        let lock = RwLock::try_new(1_u32).expect("ERESOURCE initialization should succeed");

        let read_guard = lock.read();
        assert!(lock.try_read().is_none());
        assert!(lock.try_write().is_none());
        drop(read_guard);

        let write_guard = lock.write();
        assert!(lock.try_read().is_none());
        assert!(lock.try_write().is_none());
        drop(write_guard);

        assert!(lock.try_read().is_some());
    }

    #[test]
    #[should_panic(expected = "recursive RwLock acquisition")]
    fn rw_lock_read_panics_on_recursive_acquisition() {
        let _stub_guard = lock_kernel_stubs();
        let lock = RwLock::try_new(1_u32).expect("ERESOURCE initialization should succeed");
        let _write_guard = lock.write();

        let _recursive_guard = lock.read();
    }

    #[test]
    #[should_panic(expected = "recursive RwLock acquisition")]
    fn rw_lock_write_panics_on_recursive_acquisition() {
        let _stub_guard = lock_kernel_stubs();
        let lock = RwLock::try_new(1_u32).expect("ERESOURCE initialization should succeed");
        let _read_guard = lock.read();

        let _recursive_guard = lock.write();
    }

    #[test]
    fn push_lock_read_and_write_guards_access_value() {
        let _stub_guard = lock_kernel_stubs();
        let lock = PushLock::new(1_u32);

        reset_push_lock_events();
        assert_eq!(*lock.read(), 1);
        assert_eq!(push_lock_events(), PUSH_LOCK_SHARED_EVENTS);

        reset_push_lock_events();
        {
            let mut value = lock.write();
            *value = 4;
        }
        assert_eq!(push_lock_events(), PUSH_LOCK_EXCLUSIVE_EVENTS);

        reset_push_lock_events();
        assert_eq!(*lock.read(), 4);
        assert_eq!(push_lock_events(), PUSH_LOCK_SHARED_EVENTS);
    }

    #[test]
    fn push_lock_get_mut_accesses_value_without_locking() {
        let _stub_guard = lock_kernel_stubs();
        let mut lock = PushLock::new(1_u32);

        *lock.get_mut() = 9;

        reset_push_lock_events();
        assert_eq!(*lock.read(), 9);
        assert_eq!(push_lock_events(), PUSH_LOCK_SHARED_EVENTS);
    }

    #[test]
    fn rw_spin_lock_read_and_write_guards_access_value() {
        let _stub_guard = lock_kernel_stubs();
        let lock = RwSpinLock::new(1_u32);

        reset_spin_lock_events();
        // SAFETY: The test models a caller at IRQL <= DISPATCH_LEVEL and does
        // not nest spin-lock guards.
        assert_eq!(*unsafe { lock.read() }, 1);
        assert_eq!(spin_lock_events(), SPIN_LOCK_SHARED_EVENTS);

        reset_spin_lock_events();
        {
            // SAFETY: The prior guard has been dropped, and this test does not
            // nest spin-lock guards.
            let mut value = unsafe { lock.write() };
            *value = 5;
        }
        assert_eq!(spin_lock_events(), SPIN_LOCK_EXCLUSIVE_EVENTS);

        reset_spin_lock_events();
        // SAFETY: The prior guard has been dropped, and this test does not nest
        // spin-lock guards.
        assert_eq!(*unsafe { lock.read() }, 5);
        assert_eq!(spin_lock_events(), SPIN_LOCK_SHARED_EVENTS);
    }

    #[test]
    fn rw_spin_lock_get_mut_accesses_value_without_locking() {
        let _stub_guard = lock_kernel_stubs();
        let mut lock = RwSpinLock::new(1_u32);

        *lock.get_mut() = 11;

        reset_spin_lock_events();
        // SAFETY: The test models a caller at IRQL <= DISPATCH_LEVEL and does
        // not nest spin-lock guards.
        assert_eq!(*unsafe { lock.read() }, 11);
        assert_eq!(spin_lock_events(), SPIN_LOCK_SHARED_EVENTS);
    }

    #[test]
    fn rw_spin_lock_dpc_level_guards_access_value() {
        let _stub_guard = lock_kernel_stubs();
        let lock = RwSpinLock::new(1_u32);

        reset_spin_lock_events();
        // SAFETY: The test stubs do not inspect or modify IRQL, so they model
        // the caller already running at DISPATCH_LEVEL.
        let value = unsafe { lock.read_at_dpc_level() };
        assert_eq!(*value, 1);
        drop(value);
        assert_eq!(
            spin_lock_events(),
            SPIN_LOCK_SHARED_AT_DPC_LEVEL_EVENTS
        );

        reset_spin_lock_events();
        // SAFETY: The test stubs do not inspect or modify IRQL, so they model
        // the caller already running at DISPATCH_LEVEL.
        let mut value = unsafe { lock.write_at_dpc_level() };
        *value = 13;
        drop(value);
        assert_eq!(
            spin_lock_events(),
            SPIN_LOCK_EXCLUSIVE_AT_DPC_LEVEL_EVENTS
        );

        reset_spin_lock_events();
        // SAFETY: Both DPC-level guards have been dropped, and this test does
        // not nest spin-lock guards.
        assert_eq!(*unsafe { lock.read() }, 13);
        assert_eq!(spin_lock_events(), SPIN_LOCK_SHARED_EVENTS);
    }
}
