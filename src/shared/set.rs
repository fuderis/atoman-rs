use super::{SharedGuard, SharedGuardMut, SharedItem};

use ahash::RandomState;
use once_cell::sync::OnceCell;
use std::{
    borrow::Borrow,
    collections::HashSet,
    hash::{BuildHasher, Hash, Hasher},
    sync::Arc,
};
use tokio::sync::RwLock;

/// Internal HashSet element (stores a `key` for hashing and `item`).
pub struct SharedSetItem<T> {
    pub(crate) key: Arc<T>,
    pub(crate) item: SharedItem<T>,
}

impl<T> Clone for SharedSetItem<T> {
    fn clone(&self) -> Self {
        Self {
            key: Arc::clone(&self.key),
            item: self.item.clone(),
        }
    }
}

impl<T: Hash> Hash for SharedSetItem<T> {
    #[inline]
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.key.hash(state);
    }
}

impl<T: PartialEq> PartialEq for SharedSetItem<T> {
    #[inline]
    fn eq(&self, other: &Self) -> bool {
        self.key == other.key
    }
}

impl<T: Eq> Eq for SharedSetItem<T> {}

impl<T> Borrow<T> for SharedSetItem<T> {
    #[inline]
    fn borrow(&self) -> &T {
        &self.key
    }
}

/// Total number of shards used to partition the set entries.
pub const SHARDS_COUNT: usize = 64;

/// Single shard wrapping a thread-safe hash set protected by a reader-writer lock.
pub(crate) struct Shard<T: Eq + Hash + 'static> {
    set: RwLock<HashSet<SharedSetItem<T>>>,
}

/// Inner container holding the fixed-size array of shards and the hasher builder.
pub(crate) struct SharedSetInner<T: Eq + Hash + 'static> {
    shards: [Shard<T>; SHARDS_COUNT],
    hasher_builder: RandomState,
}

/// Concurrent, sharded hash set designed for static or global state.
pub struct SharedSet<T: Eq + Hash + 'static> {
    inner: OnceCell<Arc<SharedSetInner<T>>>,
}

impl<T: Eq + Hash + 'static> ::std::default::Default for SharedSet<T> {
    fn default() -> Self {
        Self {
            inner: OnceCell::default(),
        }
    }
}

impl<T: Eq + Hash + 'static> SharedSet<T> {
    pub const fn new() -> Self {
        Self {
            inner: OnceCell::new(),
        }
    }

    fn get_or_init(&self) -> &Arc<SharedSetInner<T>> {
        self.inner.get_or_init(|| {
            let shards = std::array::from_fn(|_| Shard {
                set: RwLock::new(HashSet::new()),
            });
            Arc::new(SharedSetInner {
                shards,
                hasher_builder: RandomState::new(),
            })
        })
    }

    #[inline]
    fn get_shard<'a>(&'a self, inner: &'a SharedSetInner<T>, value: &T) -> &'a Shard<T> {
        let mut hasher = inner.hasher_builder.build_hasher();
        value.hash(&mut hasher);
        let index = (hasher.finish() as usize) % SHARDS_COUNT;
        &inner.shards[index]
    }

    /// Inserts element into the set asynchronously.
    /// Overwrites existing element if present and returns `Some(old_item)`.
    pub async fn insert(&self, value: T) -> Option<SharedItem<T>>
    where
        T: Clone,
    {
        let inner = self.get_or_init();
        let shard = self.get_shard(inner, &value);

        let key_arc = Arc::new(value.clone());
        let item = SharedItem::new(value);
        let set_item = SharedSetItem { key: key_arc, item };

        let mut guard = shard.set.write().await;
        guard.replace(set_item).map(|s| s.item)
    }

    /// Inserts element into the set only if it is not already present.
    /// (returns `Some(existing_item)` if present, or `None` if inserted successfully)
    pub async fn try_insert(&self, value: T) -> Option<SharedItem<T>>
    where
        T: Clone,
    {
        let inner = self.get_or_init();
        let shard = self.get_shard(inner, &value);

        let mut guard = shard.set.write().await;
        if let Some(existing) = guard.get(&value) {
            Some(existing.item.clone())
        } else {
            let key_arc = Arc::new(value.clone());
            let item = SharedItem::new(value);
            let set_item = SharedSetItem { key: key_arc, item };
            guard.insert(set_item);
            None
        }
    }

    /// Removes element from the set asynchronously and returns the removed item.
    pub async fn remove(&self, value: &T) -> Option<SharedItem<T>> {
        let Some(inner) = self.inner.get() else {
            return None;
        };
        let shard = self.get_shard(inner, value);

        let mut guard = shard.set.write().await;
        guard.take(value).map(|s| s.item)
    }

    /// Returns true if the set contains an element equal to `value`.
    pub async fn contains(&self, value: &T) -> bool {
        let Some(inner) = self.inner.get() else {
            return false;
        };
        let shard = self.get_shard(inner, value);
        let guard = shard.set.read().await;
        guard.contains(value)
    }

    /// Fetches `SharedItem` corresponding to the given value.
    pub async fn get(&self, value: &T) -> Option<SharedItem<T>> {
        let Some(inner) = self.inner.get() else {
            return None;
        };
        let shard = self.get_shard(inner, value);
        let guard = shard.set.read().await;
        guard.get(value).map(|s| s.item.clone())
    }

    /// Returns a vector with all elements of `SharedItem<T>`.
    pub async fn get_all(&self) -> Vec<SharedItem<T>> {
        let Some(inner) = self.inner.get() else {
            return Vec::new();
        };

        let mut items = Vec::with_capacity(self.count().await);

        for shard in &inner.shards {
            let guard = shard.set.read().await;
            items.extend(guard.iter().map(|s| s.item.clone()));
        }

        items
    }

    /// Fetches read-only guard for the value corresponding to the given element.
    pub async fn read(&self, value: &T) -> Option<SharedGuard<T>> {
        let item = self.get(value).await?;
        Some(item.read().await)
    }

    /// Fetches mutable write guard for the value corresponding to the given element.
    pub async fn write(&self, value: &T) -> Option<SharedGuardMut<T>> {
        let item = self.get(value).await?;
        Some(item.write().await)
    }

    /// Searches for entry matching the async predicate `f`.
    pub async fn find<F, Fut>(&self, f: F) -> Option<(Arc<T>, SharedItem<T>)>
    where
        F: Fn(&Arc<T>, SharedGuard<T>) -> Fut,
        Fut: std::future::Future<Output = bool>,
    {
        let inner = self.inner.get()?;

        for shard in &inner.shards {
            let len = {
                let guard = shard.set.read().await;
                guard.len()
            };

            for i in 0..len {
                let pair = {
                    let guard = shard.set.read().await;
                    guard
                        .iter()
                        .nth(i)
                        .map(|s| (Arc::clone(&s.key), s.item.clone()))
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

    /// Returns total number of elements stored across all shards.
    pub async fn count(&self) -> usize {
        let Some(inner) = self.inner.get() else {
            return 0;
        };

        let mut total = 0;
        for shard in &inner.shards {
            let guard = shard.set.read().await;
            total += guard.len();
        }
        total
    }

    /// Returns `true` if the set contains no elements.
    pub async fn is_empty(&self) -> bool {
        self.count().await == 0
    }
}
