use std::sync::Arc;
use tokio::sync::OwnedRwLockWriteGuard;

use super::StateWrap;

/// Guard transaction for state changes.
pub struct StateGuard<T: Clone + Send + Sync + 'static> {
    pub(super) _write_guard: OwnedRwLockWriteGuard<Arc<T>>,
    pub(super) wrap: Arc<StateWrap<T>>,
    pub(super) data: T,
    pub(super) counter: usize,
}

impl<T: Clone + Send + Sync + 'static> StateGuard<T> {
    /// Synchronizes changes in `ArcSwap`.
    pub fn sync(&mut self) {
        let data = Arc::new(self.data.clone());
        self.wrap.swap.store(data);
    }

    /// Synchronizes data only on every N‑th call.
    pub fn sync_n(&mut self, n: usize) {
        if n == 0 || self.counter % n == 0 {
            self.sync();
        }
        self.counter += 1;
    }
}

impl<T: Clone + Send + Sync + 'static> Drop for StateGuard<T> {
    fn drop(&mut self) {
        self.sync();
    }
}

impl<T: Clone + Send + Sync + 'static> std::ops::Deref for StateGuard<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.data
    }
}

impl<T: Clone + Send + Sync + 'static> std::ops::DerefMut for StateGuard<T> {
    fn deref_mut(&mut self) -> &mut T {
        &mut self.data
    }
}

impl<T: Clone + Send + Sync + std::fmt::Debug> std::fmt::Debug for StateGuard<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", &self.data)
    }
}

impl<T: Clone + Send + Sync + std::fmt::Display> std::fmt::Display for StateGuard<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", &self.data)
    }
}
