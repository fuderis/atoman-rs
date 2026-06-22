use super::ERR_MSG;
use crate::flag::Flag;
use crate::prelude::*;

/// The atomic state guard
pub struct StateGuard<T: Clone + Send + Sync> {
    pub(super) mutex: Arc<Mutex<Arc<T>>>,
    pub(super) swap: Arc<ArcSwapAny<Arc<T>>>,
    pub(super) data: T,
    pub(super) lock: Arc<Flag>,
    pub(super) counter: usize,
}

impl<T: Clone + Send + Sync> StateGuard<T> {
    /// Manually synchronizes the actual data with the state
    pub fn sync(&self) {
        let data = Arc::new(self.data.clone());
        *self.mutex.lock().expect(ERR_MSG) = data.clone();
        self.swap.store(data);
    }

    /// Manually synchronizes the actual data with the state
    /// (synchronizes only every N calls)
    pub fn sync_n(&mut self, n: usize) {
        if n == 0 || self.counter % n == 0 {
            self.sync();
        }
        self.counter += 1;
    }
}

impl<T: Clone + Send + Sync> ::std::ops::Drop for StateGuard<T> {
    fn drop(&mut self) {
        self.sync();
        self.lock.unlock();

        #[cfg(feature = "trace-lock")]
        {
            trace!(
                "[State<{}>] [{:?}] Unlocked",
                std::any::type_name::<T>(),
                std::thread::current().id(),
            );
        }
    }
}

impl<T: Clone + Send + Sync> ::std::ops::Deref for StateGuard<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.data
    }
}

impl<T: Clone + Send + Sync> ::std::ops::DerefMut for StateGuard<T> {
    fn deref_mut(&mut self) -> &mut T {
        &mut self.data
    }
}

impl<T: Clone + Send + Sync + ::std::fmt::Debug> ::std::fmt::Debug for StateGuard<T> {
    fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
        write!(f, "{:?}", &self.data)
    }
}

impl<T: Clone + Send + Sync + ::std::fmt::Display> ::std::fmt::Display for StateGuard<T> {
    fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
        write!(f, "{}", &self.data)
    }
}
