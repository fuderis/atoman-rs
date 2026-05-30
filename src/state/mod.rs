pub mod guard;
pub use guard::StateGuard;

pub(super) const ERR_MSG: &str = "The data has been poisoned!";

use crate::flag::Flag;
use crate::prelude::*;

/// The atomic state wrapper
#[derive(Clone)]
pub struct StateWrap<T: Default + Clone + Send + Sync> {
    mutex: Arc<Mutex<Arc<T>>>,
    swap: Arc<ArcSwapAny<Arc<T>>>,
    lock: Arc<Flag>,
}

/// The atomic state
pub struct State<T: Default + Clone + Send + Sync> {
    wrap: Lazy<Arc<StateWrap<T>>>,
}

impl<T: Default + Clone + Send + Sync> State<T> {
    /// Creates a new state
    pub const fn new() -> Self {
        Self {
            wrap: Lazy::new(|| {
                let arc_val = Arc::new(T::default());
                Arc::new(StateWrap {
                    mutex: Arc::new(Mutex::new(arc_val.clone())),
                    swap: Arc::new(ArcSwapAny::from(arc_val)),
                    lock: Arc::new(Flag::from(false)),
                })
            }),
        }
    }

    /// Returns true if data locked by some StateGuard
    pub fn is_locked(&self) -> bool {
        self.wrap.lock.is_true()
    }

    /// Waits for unlock state guard
    pub async fn wait_unlock(&self) {
        while self.is_locked() {
            self.wrap.lock.wait(false).await;
        }
    }

    /// Waits for unlock state guard (with synchronously blocking)
    pub fn blocking_wait_unlock(&self) {
        while self.is_locked() {
            self.wrap.lock.blocking_wait(false);
        }
    }

    /// Returns a state guard
    pub async fn lock(&self) -> StateGuard<T> {
        self.wait_unlock().await;
        self.dirty_lock()
    }

    /// Returns a state guard (with synchronously blocking)
    pub fn blocking_lock(&self) -> StateGuard<T> {
        self.blocking_wait_unlock();
        self.dirty_lock()
    }

    /// Returns a state guard (warning: changes not be saved if one of StateGuard is alive)
    pub fn dirty_lock(&self) -> StateGuard<T> {
        self.wrap.lock.set(true);

        StateGuard {
            mutex: self.wrap.mutex.clone(),
            swap: self.wrap.swap.clone(),
            data: self.dirty_get_cloned(),
            lock: self.wrap.lock.clone(),
            counter: 0,
        }
    }

    /// Returns a state value
    pub async fn get(&self) -> Arc<T> {
        self.wait_unlock().await;
        self.dirty_get()
    }

    /// Returns a state value (with synchronously blocking)
    pub fn blocking_get(&self) -> Arc<T> {
        self.blocking_wait_unlock();
        self.dirty_get()
    }

    /// Returns a state value (warning: may not contain actual data)
    pub fn dirty_get(&self) -> Arc<T> {
        self.wrap.swap.load_full()
    }

    /// Returns a clone of state value
    pub async fn get_cloned(&self) -> T {
        self.wait_unlock().await;
        self.dirty_get_cloned()
    }

    /// Returns a clone of state value (with synchronously blocking)
    pub fn blocking_get_cloned(&self) -> T {
        self.blocking_wait_unlock();
        self.dirty_get_cloned()
    }

    /// Returns a clone of state value (warning: may not contain actual data)
    pub fn dirty_get_cloned(&self) -> T {
        self.wrap.swap.load_full().as_ref().clone()
    }

    /// Sets a new value to state
    pub async fn set(&self, value: T) {
        self.wait_unlock().await;
        self.wrap.lock.set(true);

        self.dirty_set(value);
        self.wrap.lock.set(false);
    }

    /// Sets a new value to state (with synchronously blocking)
    pub fn blocking_set(&self, value: T) {
        self.blocking_wait_unlock();
        self.wrap.lock.set(true);

        self.dirty_set(value);
        self.wrap.lock.set(false);
    }

    /// Sets a new value to state (warning: changes not be saved if one of StateGuard is alive)
    pub fn dirty_set(&self, value: T) {
        let new_data = Arc::new(value);
        let mut lock = self.wrap.mutex.lock().unwrap();
        *lock = new_data.clone();
        self.wrap.swap.store(new_data);
    }

    /// Writes data directly
    pub async fn map(&self, f: impl FnOnce(&mut T)) {
        let mut guard = self.lock().await;
        let mut data = (*guard).clone();

        f(&mut data);
        *guard = data;
    }

    /// Writes data directrly (with synchronously blocking)
    pub fn blocking_map(&self, f: impl FnOnce(&mut T)) {
        let mut guard = self.blocking_lock();
        let mut data = (*guard).clone();
        f(&mut data);
        *guard = data;
    }

    /// Writes data directly (warning: changes not be saved if one of StateGuard is alive)
    pub fn dirty_map(&self, f: impl FnOnce(&mut T)) {
        let mut guard = self.dirty_lock();
        let mut data = (*guard).clone();

        f(&mut data);
        *guard = data;
    }
}

impl<T: Default + Clone + Send + Sync> ::std::default::Default for State<T> {
    fn default() -> Self {
        let this = Self::new();
        this.dirty_set(Default::default());
        this
    }
}

impl<T: Default + Clone + Send + Sync> ::std::convert::From<T> for State<T> {
    fn from(value: T) -> Self {
        let this = Self::new();
        this.dirty_set(value);
        this
    }
}

impl<T: Default + Clone + Send + Sync + Debugging> ::std::fmt::Debug for State<T> {
    fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
        write!(f, "{:?}", &self.dirty_get())
    }
}

impl<T: Default + Clone + Send + Sync + Displaying> ::std::fmt::Display for State<T> {
    fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
        write!(f, "{}", &self.dirty_get())
    }
}
