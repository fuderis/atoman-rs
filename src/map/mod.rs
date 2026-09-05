pub mod item;
pub use item::SharedItem;

pub mod guard;
pub use guard::{SharedGuard, SharedGuardMut};

use ahash::RandomState;
use once_cell::sync::OnceCell;
use std::{
    collections::HashMap,
    hash::{BuildHasher, Hash, Hasher},
    sync::Arc,
};

/// Total number of shards used to partition the map entries.
pub const SHARDS_COUNT: usize = 64;

/// Single shard wrapping a thread-safe hash map protected by a reader-writer lock.
pub(crate) struct Shard<K: Eq + Hash + 'static, V> {
    map: tokio::sync::RwLock<HashMap<Arc<K>, SharedItem<V>>>,
}

/// Inner container holding the fixed-size array of shards and the hasher builder.
pub(crate) struct SharedMapInner<K: Eq + Hash + 'static, V> {
    shards: [Shard<K, V>; SHARDS_COUNT],
    hasher_builder: RandomState,
}

/// Concurrent, sharded hash map designed for static or global state.
pub struct SharedMap<K: Eq + Hash + 'static, V: 'static> {
    inner: OnceCell<Arc<SharedMapInner<K, V>>>,
}

impl<K: Eq + Hash + 'static, V: 'static> ::std::default::Default for SharedMap<K, V> {
    fn default() -> Self {
        Self {
            inner: OnceCell::default(),
        }
    }
}

impl<K: Eq + Hash + 'static, V: 'static> SharedMap<K, V> {
    pub const fn new() -> Self {
        Self {
            inner: OnceCell::new(),
        }
    }

    fn get_or_init(&self) -> &Arc<SharedMapInner<K, V>> {
        self.inner.get_or_init(|| {
            let shards = std::array::from_fn(|_| Shard {
                map: tokio::sync::RwLock::new(HashMap::new()),
            });
            Arc::new(SharedMapInner {
                shards,
                hasher_builder: RandomState::new(),
            })
        })
    }

    #[inline]
    fn get_shard<'a>(&'a self, inner: &'a SharedMapInner<K, V>, key: &K) -> &'a Shard<K, V> {
        let mut hasher = inner.hasher_builder.build_hasher();
        key.hash(&mut hasher);
        let index = (hasher.finish() as usize) % SHARDS_COUNT;
        &inner.shards[index]
    }

    /// Inserts key-value pair into the map asynchronously.
    pub async fn insert(&self, key: K, value: V) {
        let inner = self.get_or_init();
        let shard = self.get_shard(inner, &key);

        let key_arc = Arc::new(key);
        let item = SharedItem::new(value);

        let mut guard = shard.map.write().await;
        guard.insert(key_arc, item);
    }

    /// Removes key from the map asynchronously and returns the removed item.
    pub async fn remove(&self, key: &K) -> Option<SharedItem<V>> {
        let inner = self.get_or_init();
        let shard = self.get_shard(inner, key);

        let mut guard = shard.map.write().await;
        guard.remove(key)
    }

    /// Fetches `SharedItem` for the value corresponding to the given key.
    pub async fn get(&self, key: &K) -> Option<SharedItem<V>> {
        let inner = self.get_or_init();
        let shard = self.get_shard(inner, key);
        let guard = shard.map.read().await;
        guard.get(key).cloned()
    }

    /// Fetches read-only guard for the value corresponding to the given key.
    pub async fn read(&self, key: &K) -> Option<SharedGuard<V>> {
        let item = self.get(key).await?;
        Some(item.read().await)
    }

    /// Fetches mutable write guard for the value corresponding to the given key.
    pub async fn write(&self, key: &K) -> Option<SharedGuardMut<V>> {
        let item = self.get(key).await?;
        Some(item.write().await)
    }

    /// Searches for entry matching the async predicate `f`.
    pub async fn find<F, Fut>(&self, f: F) -> Option<(Arc<K>, SharedItem<V>)>
    where
        F: Fn(&Arc<K>, SharedGuard<V>) -> Fut,
        Fut: std::future::Future<Output = bool>,
    {
        let inner = self.inner.get()?;

        for shard in &inner.shards {
            let len = {
                let guard = shard.map.read().await;
                guard.len()
            };

            for i in 0..len {
                let pair = {
                    let guard = shard.map.read().await;
                    guard.iter().nth(i).map(|(k, v)| (Arc::clone(k), v.clone()))
                };

                let Some((key, item)) = pair else {
                    continue;
                };

                let guard = item.read().await;
                if f(&key, guard).await {
                    return Some((key, item));
                }
            }
        }

        None
    }

    /// Collects all items and returns as `HashMap<Arc<K>, SharedItem<V>>`.
    pub async fn to_hash(&self) -> HashMap<Arc<K>, SharedItem<V>> {
        let Some(inner) = self.inner.get() else {
            return HashMap::new();
        };

        let mut items = HashMap::<Arc<K>, SharedItem<V>>::with_capacity(self.count().await);

        for shard in &inner.shards {
            let guard = shard.map.read().await;
            for (k, v) in guard.iter() {
                items.insert(Arc::clone(k), v.clone());
            }
        }

        items
    }

    /// Returns total number of key-value pairs stored across all shards.
    pub async fn count(&self) -> usize {
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
        self.count().await == 0
    }
}
