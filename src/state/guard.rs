use arc_swap::ArcSwapAny;
use std::sync::Arc;
use tokio::sync::RwLockWriteGuard;

/// Guard transaction for state changes.
///
/// Holds the write lock on the table during the modification of the local copy,
/// and when `sync()` or `Drop` is called, synchronizes the data with `ArcSwap`.
pub struct StateGuard<'a, T: Clone + Send + Sync + 'static> {
    pub(super) _write_guard: RwLockWriteGuard<'a, Arc<T>>,
    pub(super) swap: &'a ArcSwapAny<Arc<T>>,
    pub(super) data: T,
    pub(super) counter: usize,
}

impl<'a, T: Clone + Send + Sync + 'static> StateGuard<'a, T> {
    /// Synchronizes changes in `ArcSwap`.
    pub fn sync(&mut self) {
        let data = Arc::new(self.data.clone());
        self.swap.store(data);
    }

    /// Synchronizes data only on every N‑th call.
    pub fn sync_n(&mut self, n: usize) {
        if n == 0 || self.counter % n == 0 {
            self.sync();
        }
        self.counter += 1;
    }
}

impl<'a, T: Clone + Send + Sync + 'static> Drop for StateGuard<'a, T> {
    fn drop(&mut self) {
        self.sync();

        #[cfg(feature = "trace-lock")]
        {
            println!(
                "[State<{}>] [{:?}] Unlocked",
                std::any::type_name::<T>(),
                std::thread::current().id(),
            );
        }
    }
}

impl<'a, T: Clone + Send + Sync + 'static> std::ops::Deref for StateGuard<'a, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.data
    }
}

impl<'a, T: Clone + Send + Sync + 'static> std::ops::DerefMut for StateGuard<'a, T> {
    fn deref_mut(&mut self) -> &mut T {
        &mut self.data
    }
}

impl<'a, T: Clone + Send + Sync + std::fmt::Debug> std::fmt::Debug for StateGuard<'a, T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", &self.data)
    }
}

impl<'a, T: Clone + Send + Sync + std::fmt::Display> std::fmt::Display for StateGuard<'a, T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", &self.data)
    }
}
