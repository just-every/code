#![cfg(feature = "NSLock")]
use crate::{NSLock, NSLocking};

#[test]
fn lock_unlock() {
    let lock = NSLock::new();
    unsafe {
        lock.lock();
        assert!(!lock.tryLock());
        lock.unlock();
        assert!(lock.tryLock());
        lock.unlock();
    }
}
