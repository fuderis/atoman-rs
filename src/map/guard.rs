use std::ops::{Deref, DerefMut};
use tokio::sync::{OwnedRwLockReadGuard, OwnedRwLockWriteGuard};

/// Asynchronous RAII-guard for reading
pub struct MapGuard<V: 'static> {
    pub(crate) guard: OwnedRwLockReadGuard<V>,
}

impl<V: 'static> Deref for MapGuard<V> {
    type Target = V;

    #[inline]
    fn deref(&self) -> &Self::Target {
        &self.guard
    }
}

/// Asynchronous RAII-guard for writing
pub struct MapGuardMut<V: 'static> {
    pub(crate) guard: OwnedRwLockWriteGuard<V>,
}

impl<V: 'static> Deref for MapGuardMut<V> {
    type Target = V;

    #[inline]
    fn deref(&self) -> &Self::Target {
        &self.guard
    }
}

impl<V: 'static> DerefMut for MapGuardMut<V> {
    #[inline]
    fn deref_mut(&mut self) -> &mut V {
        &mut self.guard
    }
}
