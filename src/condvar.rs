#![cfg(target_os = "linux")]

use std::{
    ptr::null,
    sync::atomic::{AtomicI32, Ordering},
};

///
/// This Condvar is meant to enable shared memory writers to signal to shared memory readers after a write.
/// Standard rust Condvars cannot be used in such a context as they specify the FUTEX_PRIVATE_FLAG
///
#[derive(Debug)]
pub struct Condvar {
    inner: Futex,
}

#[derive(Debug)]
pub enum ErrorCode {
    WaitInterrupted,
    InvalidWakeArguments,
}

impl Condvar {
    ///
    /// Create a new futex based Condvar.
    ///
    pub fn new() -> Self {
        Condvar {
            inner: Futex {
                value: AtomicI32::new(0),
            },
        }
    }

    ///
    /// The current thread will wait for this Condvar to be realized
    ///
    /// ```
    ///  use std::thread;
    ///  use std::sync::Arc;
    ///  use rshm::condvar::Condvar;
    ///
    ///  let condvar = Arc::new(Condvar::new());
    ///  let condvar_clone = condvar.clone();
    ///  let waiting_thread = thread::spawn(move || {
    ///     condvar.wait().unwrap();
    ///     true
    ///  });
    ///  let waking_thread = thread::spawn(move || {
    ///     let mut result = 0;
    ///     while result == 0 {
    ///      result = condvar_clone.notify_all().unwrap();
    ///     }
    ///     result
    ///  });
    ///  assert!(waiting_thread.join().unwrap());
    ///  assert_eq!(1, waking_thread.join().unwrap());
    /// ```
    ///
    pub fn wait(&self) -> Result<(), ErrorCode> {
        unsafe { self.inner.wait() }
    }

    ///
    /// Notifies all waiting threads that the Condvar is realized
    ///
    /// ```
    ///  use std::thread;
    ///  use std::sync::Arc;
    ///  use rshm::condvar::Condvar;
    ///
    ///  let condvar = Arc::new(Condvar::new());
    ///  let condvar_clone1 = condvar.clone();
    ///  let condvar_clone2 = condvar.clone();
    ///  let waiting_thread1 = thread::spawn(move || {
    ///     condvar_clone1.wait().unwrap();
    ///     true
    ///  });
    ///  let waiting_thread2 = thread::spawn(move || {
    ///     condvar_clone2.wait().unwrap();
    ///     true
    ///  });
    ///  let waking_thread = thread::spawn(move || {
    ///     std::thread::sleep(std::time::Duration::from_secs(1));
    ///     condvar.notify_all().unwrap()
    ///  });
    ///  assert!(waiting_thread1.join().unwrap());
    ///  assert!(waiting_thread2.join().unwrap());
    ///  assert_eq!(2, waking_thread.join().unwrap());
    /// ```
    ///
    pub fn notify_all(&self) -> Result<i32, ErrorCode> {
        unsafe { self.inner.wake(libc::INT_MAX) }
    }
}

#[derive(Debug)]
struct Futex {
    value: AtomicI32,
}

impl Futex {
    unsafe fn wait(&self) -> Result<(), ErrorCode> {
        let expected_value = self.value.load(Ordering::Acquire);
        while expected_value >= self.value.load(Ordering::Acquire) {
            let result = libc::syscall(
                libc::SYS_futex,
                &self.value,
                libc::FUTEX_WAIT,
                expected_value,
                null() as *const libc::timespec,
                null() as *const AtomicI32,
                0,
            ) as i32;
            if result == libc::EINTR {
                return Err(ErrorCode::WaitInterrupted);
            }
        }
        Ok(())
    }

    unsafe fn wake(&self, count: i32) -> Result<i32, ErrorCode> {
        self.value.fetch_add(1, Ordering::Release);
        let result = libc::syscall(
            libc::SYS_futex,
            &self.value,
            libc::FUTEX_WAKE,
            count,
            null() as *const libc::timespec,
            null() as *const AtomicI32,
            0,
        ) as i32;
        if result == libc::EINVAL {
            Err(ErrorCode::InvalidWakeArguments)
        } else {
            Ok(result)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Futex;
    use std::sync::atomic::AtomicI32;
    use std::sync::Arc;
    use std::thread;

    #[test]
    fn futex_wake_is_woken_up() {
        let futex = Arc::new(Futex {
            value: AtomicI32::new(0),
        });
        let futex_clone = futex.clone();
        let waiting_thread = thread::spawn(move || {
            unsafe { futex.wait().unwrap() };
            true
        });
        let waking_thread = thread::spawn(move || {
            let mut result = 0;
            while result == 0 {
                result = unsafe { futex_clone.wake(1).unwrap() };
            }
            result
        });
        assert!(waiting_thread.join().unwrap());
        assert_eq!(1, waking_thread.join().unwrap());
    }
}
