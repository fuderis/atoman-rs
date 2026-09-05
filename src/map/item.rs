use super::{SharedGuard, SharedGuardMut};

use std::sync::Arc;
use tokio::sync::RwLock;

/// Isolated wrapper over the map element.
///
/// Hides the detailed structure with `Arc<RwLock<V>>`.
pub struct SharedItem<V> {
    inner: Arc<RwLock<V>>,
}

impl<V> Clone for SharedItem<V> {
    fn clone(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
        }
    }
}

impl<V> SharedItem<V> {
    pub(crate) fn new(value: V) -> Self {
        Self {
            inner: Arc::new(RwLock::new(value)),
        }
    }

    /// Provides access to the internal `Arc<RwLock<V>>`
    /// (if a low‑level access is needed).
    pub fn into_inner(self) -> Arc<RwLock<V>> {
        self.inner
    }
}

impl<V: 'static> SharedItem<V> {
    /// Captures read lock on the value (asynchronous).
    pub async fn read(&self) -> SharedGuard<V> {
        let lock = Arc::clone(&self.inner);
        let guard = lock.read_owned().await;
        SharedGuard { guard }
    }

    /// Captures write lock on the value (asynchronous).
    pub async fn write(&self) -> SharedGuardMut<V> {
        let lock = Arc::clone(&self.inner);
        let guard = lock.write_owned().await;
        SharedGuardMut { guard }
    }
}
