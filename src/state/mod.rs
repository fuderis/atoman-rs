pub mod guard;
pub use guard::StateGuard;

use arc_swap::ArcSwapAny;
use once_cell::sync::OnceCell;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Internal state wrapper.
///
/// `RwLock<Arc<T>>` protects the modification process,
/// while `ArcSwapAny` provides instant lock‑free reading (lock‑free read).
pub struct StateWrap<T: Clone + Send + Sync> {
    lock: Arc<RwLock<Arc<T>>>,
    swap: ArcSwapAny<Arc<T>>,
}

/// Atomic shared state with lock-free reading via `ArcSwap`
pub struct State<T: Clone + Send + Sync + 'static> {
    wrap: OnceCell<Arc<StateWrap<T>>>,
    init_fn: fn() -> T,
}

impl<T: Clone + Send + Sync> State<T> {
    /// Creates new state with custom initializator
    pub const fn new(init_fn: fn() -> T) -> Self {
        Self {
            wrap: OnceCell::new(),
            init_fn,
        }
    }

    /// Lazy initialization on first access
    fn get_or_init(&self) -> &Arc<StateWrap<T>> {
        self.wrap.get_or_init(|| {
            let initial_value = Arc::new((self.init_fn)());
            Arc::new(StateWrap {
                lock: Arc::new(RwLock::new(initial_value.clone())),
                swap: ArcSwapAny::from(initial_value),
            })
        })
    }

    /// Returns state guard asynchronously
    pub async fn lock(&self) -> StateGuard<T> {
        let wrap = self.get_or_init().clone();
        let lock_arc = wrap.lock.clone();
        let write_guard = lock_arc.write_owned().await;

        StateGuard {
            _write_guard: write_guard,
            wrap,
            data: self.get_dirty_cloned(),
            counter: 0,
        }
    }

    // /// Returns state guard synchronously (blocking current thread)
    // pub fn blocking_lock(&self) -> StateGuard<T> {
    //     let wrap = self.get_or_init().clone();
    //     let lock_arc = wrap.lock.clone();
    //     // Передаем Arc напрямую (без разыменования *), чтобы вызывать метод self: Arc<Self>
    //     let write_guard = RwLock::blocking_write_owned(lock_arc);

    //     StateGuard {
    //         _write_guard: write_guard,
    //         wrap,
    //         data: self.get_dirty_cloned(),
    //         counter: 0,
    //     }
    // }

    /// Returns state value (wait until active write transaction finishes)
    pub async fn get(&self) -> Arc<T> {
        let wrap = self.get_or_init();
        let _read_guard = wrap.lock.read().await;
        wrap.swap.load_full()
    }

    /// Returns state value synchronously (blocking until write finishes)
    pub fn blocking_get(&self) -> Arc<T> {
        let wrap = self.get_or_init();
        let _read_guard = wrap.lock.blocking_read();
        wrap.swap.load_full()
    }

    /// Returns state value instantly without checking locks (Lock-free)
    #[inline]
    pub fn get_dirty(&self) -> Arc<T> {
        self.get_or_init().swap.load_full()
    }

    /// Returns clone of state value (wait until active write finishes)
    pub async fn get_cloned(&self) -> T {
        self.get().await.as_ref().clone()
    }

    /// Returns clone of state value synchronously
    pub fn blocking_get_cloned(&self) -> T {
        self.blocking_get().as_ref().clone()
    }

    /// Returns clone of state value instantly without locks
    #[inline]
    pub fn get_dirty_cloned(&self) -> T {
        self.get_dirty().as_ref().clone()
    }

    /// Sets new value to state asynchronously
    pub async fn set(&self, value: T) {
        let wrap = self.get_or_init();
        let mut write_guard = wrap.lock.write().await;

        let new_data = Arc::new(value);
        *write_guard = new_data.clone();
        wrap.swap.store(new_data);
    }

    /// Sets new value to state synchronously
    pub fn blocking_set(&self, value: T) {
        let wrap = self.get_or_init();
        let mut write_guard = wrap.lock.blocking_write();

        let new_data = Arc::new(value);
        *write_guard = new_data.clone();
        wrap.swap.store(new_data);
    }

    /// Sets new value without acquiring a lock (unsafe for concurrency)
    pub fn set_dirty(&self, value: T) {
        let new_data = Arc::new(value);
        self.get_or_init().swap.store(new_data);
    }
}

impl<T: Clone + Send + Sync + 'static> Clone for State<T> {
    fn clone(&self) -> Self {
        let wrap = self.get_or_init();

        let new_wrap = OnceCell::new();
        let _ = new_wrap.set(Arc::clone(wrap));

        Self {
            wrap: new_wrap,
            init_fn: self.init_fn,
        }
    }
}

impl<T: Default + Clone + Send + Sync + 'static> State<T> {
    pub const fn default() -> Self {
        Self {
            wrap: OnceCell::new(),
            init_fn: T::default,
        }
    }
}

impl<T: Default + Clone + Send + Sync> Default for State<T> {
    fn default() -> Self {
        Self::default()
    }
}

impl<T: Default + Clone + Send + Sync> From<T> for State<T> {
    fn from(data: T) -> Self {
        let this = Self::default();
        this.set_dirty(data);
        this
    }
}

impl<T: Clone + Send + Sync + std::fmt::Debug> std::fmt::Debug for State<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", &self.get_dirty())
    }
}

impl<T: Clone + Send + Sync + std::fmt::Display> std::fmt::Display for State<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", &self.get_dirty())
    }
}
