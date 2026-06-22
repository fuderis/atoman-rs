use crate::prelude::*;
use std::time::{Duration, Instant};

/// The atomic flag wrapper
#[derive(Clone)]
pub struct FlagWrap {
    state: Arc<AtomicBool>,
    notify: Arc<Notify>,
}

/// The atomic flag for concurrent locks
pub struct Flag {
    wrap: Lazy<Arc<FlagWrap>>,
}

impl Flag {
    /// Creates a new flag
    pub const fn new() -> Self {
        Self {
            wrap: Lazy::new(|| {
                Arc::new(FlagWrap {
                    state: Arc::new(AtomicBool::new(false)),
                    notify: Arc::new(Notify::new()),
                })
            }),
        }
    }

    /// Returns true if flag is locked
    pub fn is_locked(&self) -> bool {
        self.wrap.state.load(Ordering::Acquire)
    }

    /// Try to lock capture without waiting
    /// (returns `true` if flag successfully locked)
    pub fn try_lock(&self) -> bool {
        self.wrap
            .state
            .compare_exchange(
                false, // wait for unlock
                true,  // locking
                Ordering::Acquire,
                Ordering::Relaxed,
            )
            .is_ok()
    }

    /// Releases the lock and notify the waiting threads/tasks
    pub fn unlock(&self) {
        self.wrap.state.store(false, Ordering::Release);
        self.wrap.notify.notify_waiters();
    }

    /// Asynchronously waits for the release and capture the lock
    pub async fn lock(&self) {
        while !self.try_lock() {
            self.wrap.notify.notified().await;
        }
    }

    /// Synchronously blocks the thread until the lock is released and captured
    pub fn blocking_lock(&self) {
        while !self.try_lock() {
            std::thread::sleep(Duration::from_micros(50));
        }
    }

    /// Synchronously waits for lock capture with timeout
    /// (returns `true` if lock successfully captured)
    pub fn blocking_lock_timeout(&self, timeout: Duration) -> bool {
        let deadline = Instant::now() + timeout;

        while !self.try_lock() {
            if Instant::now() > deadline {
                return false; // timeout
            }
            std::thread::sleep(Duration::from_micros(50));
        }
        true
    }
}

impl ::std::default::Default for Flag {
    fn default() -> Self {
        Self::new()
    }
}

impl ::std::fmt::Debug for Flag {
    fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
        write!(f, "{:?}", &self.is_locked())
    }
}

impl ::std::fmt::Display for Flag {
    fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
        write!(f, "{}", &self.is_locked())
    }
}

impl ::std::cmp::Eq for Flag {}

impl ::std::cmp::PartialEq for Flag {
    fn eq(&self, other: &Self) -> bool {
        self.is_locked() == other.is_locked()
    }
}

impl ::std::cmp::PartialEq<bool> for Flag {
    fn eq(&self, other: &bool) -> bool {
        &self.is_locked() == other
    }
}

#[allow(clippy::from_over_into)]
impl ::std::convert::Into<bool> for Flag {
    fn into(self) -> bool {
        self.is_locked()
    }
}
