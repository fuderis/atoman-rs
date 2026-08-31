pub mod guard;
pub use guard::{MapGuard, MapGuardMut};

use once_cell::sync::OnceCell;
use std::collections::HashMap;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::sync::Arc;
use tokio::sync::RwLock;

/// The total number of shards used to partition the map entries.
pub const SHARDS_COUNT: usize = 64;

/// A single shard wrapping a thread-safe hash map protected by a reader-writer lock.
pub(crate) struct Shard<K: Eq + Hash + 'static, V> {
    map: RwLock<HashMap<Arc<K>, Arc<RwLock<V>>>>,
}

/// Inner container holding the fixed-size array of shards.
pub(crate) struct MapInner<K: Eq + Hash + 'static, V> {
    shards: [Shard<K, V>; SHARDS_COUNT],
}

/// A concurrent, sharded hash map designed for static or global state.
pub struct Map<K: Eq + Hash + 'static, V: 'static> {
    inner: OnceCell<Arc<MapInner<K, V>>>,
}

impl<K: Eq + Hash + 'static, V: 'static> Map<K, V> {
    /// Creates a new, uninitialized sharded map.
    pub const fn new() -> Self {
        Self {
            inner: OnceCell::new(),
        }
    }

    /// Lazily initializes and returns a reference to the inner sharded map structure.
    fn get_or_init(&self) -> &Arc<MapInner<K, V>> {
        self.inner.get_or_init(|| {
            let shards = std::array::from_fn(|_| Shard {
                map: RwLock::new(HashMap::new()),
            });
            Arc::new(MapInner { shards })
        })
    }

    /// Calculates the shard index for the provided key and returns a reference to the target shard.
    #[inline]
    fn get_shard<'a>(&'a self, inner: &'a MapInner<K, V>, key: &K) -> &'a Shard<K, V> {
        let mut hasher = DefaultHasher::new();
        key.hash(&mut hasher);
        let index = (hasher.finish() as usize) % SHARDS_COUNT;
        &inner.shards[index]
    }

    /// Inserts a key-value pair into the map asynchronously.
    pub async fn insert(&self, key: K, value: V) {
        let inner = self.get_or_init();
        let shard = self.get_shard(inner, &key);

        let key_arc = Arc::new(key);
        let item = Arc::new(RwLock::new(value));

        let mut guard = shard.map.write().await;
        guard.insert(key_arc, item);
    }

    /// Removes a key from the map asynchronously and returns the removed value wrapped in an [`Arc<RwLock<V>>`].
    pub async fn remove(&self, key: &K) -> Option<Arc<RwLock<V>>> {
        let inner = self.get_or_init();
        let shard = self.get_shard(inner, key);

        let mut guard = shard.map.write().await;
        guard.remove(key)
    }

    /// Fetches a read-only guard for the value corresponding to the given key asynchronously.
    pub async fn read(&self, key: &K) -> Option<MapGuard<V>> {
        let inner = self.get_or_init();
        let item = {
            let shard = self.get_shard(inner, key);
            let guard = shard.map.read().await;
            guard.get(key).cloned()?
        };

        let guard = item.read_owned().await;
        Some(MapGuard { guard })
    }

    /// Fetches a mutable write guard for the value corresponding to the given key asynchronously.
    pub async fn write(&self, key: &K) -> Option<MapGuardMut<V>> {
        let inner = self.get_or_init();
        let item = {
            let shard = self.get_shard(inner, key);
            let guard = shard.map.read().await;
            guard.get(key).cloned()?
        };

        let guard = item.write_owned().await;
        Some(MapGuardMut { guard })
    }

    /// Searches for an entry matching the predicate `f`.
    /// Passes `Arc<K>` and `MapGuard<V>` to the predicate.
    pub async fn find<F, Fut>(&self, f: F) -> Option<(Arc<K>, Arc<RwLock<V>>)>
    where
        F: Fn(&Arc<K>, MapGuard<V>) -> Fut,
        Fut: std::future::Future<Output = bool>,
    {
        let inner = self.inner.get()?;

        for shard in &inner.shards {
            let items: Vec<_> = {
                let guard = shard.map.read().await;
                guard
                    .iter()
                    .map(|(k, v)| (Arc::clone(k), Arc::clone(v)))
                    .collect()
            };

            for (key, arc_lock) in items {
                let guard = arc_lock.clone().read_owned().await;
                if f(&key, MapGuard { guard }).await {
                    return Some((key, arc_lock));
                }
            }
        }

        None
    }

    /// Collects all items and returns as `HashMap<Arc<K>, Arc<RwLock<V>>>`.
    pub async fn to_hash(&self) -> HashMap<Arc<K>, Arc<RwLock<V>>> {
        let Some(inner) = self.inner.get() else {
            return HashMap::new();
        };

        let mut items = HashMap::with_capacity(self.len().await);
        for shard in &inner.shards {
            let guard = shard.map.read().await;
            for (k, v) in guard.iter() {
                items.insert(Arc::clone(k), Arc::clone(v));
            }
        }

        items
    }

    /// Returns the total number of key-value pairs stored across all shards.
    pub async fn len(&self) -> usize {
        let Some(inner) = self.inner.get() else {
            return 0;
        };

        let mut total = 0;
        for shard in &inner.shards {
            let guard = shard.map.read().await;
            total += guard.len();
        }
        total
    }

    /// Returns `true` if the map contains no elements.
    pub async fn is_empty(&self) -> bool {
        self.len().await == 0
    }
}

impl<K: Eq + Hash + 'static, V: 'static> Default for Map<K, V> {
    fn default() -> Self {
        Self::new()
    }
}
