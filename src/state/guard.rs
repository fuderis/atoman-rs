use arc_swap::ArcSwapAny;
use std::ops::{Deref, DerefMut};
use std::sync::Arc;
use tokio::sync::OwnedMutexGuard;

/// Guard transaction for state changes with lazy Copy-On-Write (COW).
pub struct StateGuard<T: Clone + Send + Sync + 'static> {
    pub(super) _guard: OwnedMutexGuard<()>,
    pub(super) swap: Arc<ArcSwapAny<Arc<T>>>,
    pub(super) current_arc: Arc<T>,
    pub(super) mutated_data: Option<T>,
    pub(super) counter: usize,
}

impl<T: Clone + Send + Sync + 'static> StateGuard<T> {
    /// Returns `true` if state was modified during this guard transaction.
    #[inline]
    pub fn is_modified(&self) -> bool {
        self.mutated_data.is_some()
    }

    /// Discards any uncommitted mutations made during this guard context.
    #[inline]
    pub fn cancel(&mut self) {
        self.mutated_data = None;
    }

    /// Synchronizes changes in `ArcSwap` if data was modified.
    pub fn sync(&mut self) {
        if let Some(ref data) = self.mutated_data {
            let new_arc = Arc::new(data.clone());
            self.swap.store(new_arc.clone());
            self.current_arc = new_arc;
        }
    }

    /// Synchronizes data only on every N‑th call (only if modified).
    pub fn sync_n(&mut self, n: usize) {
        if self.is_modified() && (n == 0 || self.counter % n == 0) {
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

impl<T: Clone + Send + Sync + 'static> Deref for StateGuard<T> {
    type Target = T;

    #[inline]
    fn deref(&self) -> &Self::Target {
        if let Some(ref data) = self.mutated_data {
            data
        } else {
            &self.current_arc
        }
    }
}

impl<T: Clone + Send + Sync + 'static> DerefMut for StateGuard<T> {
    #[inline]
    fn deref_mut(&mut self) -> &mut T {
        if self.mutated_data.is_none() {
            self.mutated_data = Some((*self.current_arc).clone());
        }
        self.mutated_data.as_mut().unwrap()
    }
}

impl<T: Clone + Send + Sync + std::fmt::Debug> std::fmt::Debug for StateGuard<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self.deref())
    }
}

impl<T: Clone + Send + Sync + std::fmt::Display> std::fmt::Display for StateGuard<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.deref())
    }
}
