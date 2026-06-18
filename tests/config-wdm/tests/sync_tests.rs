// Copyright (c) Microsoft Corporation
// License: MIT OR Apache-2.0

#[cfg(test)]
mod tests {
    use wdk::sync::{PushLock, RwLock, RwSpinLock};
    #[allow(unused_imports)]
    use wdk_sys::test_stubs as _;

    #[test]
    fn rw_lock_read_and_write_guards_access_value() {
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
        let mut lock = RwLock::try_new(1_u32).expect("ERESOURCE initialization should succeed");

        *lock.get_mut() = 7;

        assert_eq!(*lock.read(), 7);
    }

    #[test]
    fn push_lock_read_and_write_guards_access_value() {
        let lock = PushLock::new(1_u32);

        assert_eq!(*lock.read(), 1);

        {
            let mut value = lock.write();
            *value = 4;
        }

        assert_eq!(*lock.read(), 4);
    }

    #[test]
    fn push_lock_get_mut_accesses_value_without_locking() {
        let mut lock = PushLock::new(1_u32);

        *lock.get_mut() = 9;

        assert_eq!(*lock.read(), 9);
    }

    #[test]
    fn rw_spin_lock_read_and_write_guards_access_value() {
        let lock = RwSpinLock::new(1_u32);

        assert_eq!(*lock.read(), 1);

        {
            let mut value = lock.write();
            *value = 5;
        }

        assert_eq!(*lock.read(), 5);
    }

    #[test]
    fn rw_spin_lock_get_mut_accesses_value_without_locking() {
        let mut lock = RwSpinLock::new(1_u32);

        *lock.get_mut() = 11;

        assert_eq!(*lock.read(), 11);
    }
}
