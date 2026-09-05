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
    lock: RwLock<Arc<T>>,
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
                lock: RwLock::new(initial_value.clone()),
                swap: ArcSwapAny::from(initial_value),
            })
        })
    }

    /// Returns state guard asynchronously
    #[track_caller]
    pub fn lock(&self) -> impl std::future::Future<Output = StateGuard<'_, T>> {
        #[cfg(feature = "trace-lock")]
        let caller = std::panic::Location::caller();

        async move {
            #[cfg(feature = "trace-lock")]
            self.trace_attempt(caller);

            let wrap = self.get_or_init();
            let write_guard = wrap.lock.write().await;

            #[cfg(feature = "trace-lock")]
            self.trace_success(caller);

            StateGuard {
                _write_guard: write_guard,
                swap: &wrap.swap,
                data: self.dirty_get_cloned(),
                counter: 0,
            }
        }
    }

    /// Returns state guard synchronously (blocking current thread)
    #[track_caller]
    pub fn blocking_lock(&self) -> StateGuard<'_, T> {
        #[cfg(feature = "trace-lock")]
        let caller = std::panic::Location::caller();
        #[cfg(feature = "trace-lock")]
        self.trace_attempt(caller);

        let wrap = self.get_or_init();
        let write_guard = wrap.lock.blocking_write();

        #[cfg(feature = "trace-lock")]
        self.trace_success(caller);

        StateGuard {
            _write_guard: write_guard,
            swap: &wrap.swap,
            data: self.dirty_get_cloned(),
            counter: 0,
        }
    }

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
    pub fn dirty_get(&self) -> Arc<T> {
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
    pub fn dirty_get_cloned(&self) -> T {
        self.dirty_get().as_ref().clone()
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
    pub fn dirty_set(&self, value: T) {
        let new_data = Arc::new(value);
        self.get_or_init().swap.store(new_data);
    }
}

#[cfg(feature = "trace-lock")]
impl<T: Clone + Send + Sync> State<T> {
    #[inline(always)]
    fn trace_attempt(&self, caller: &std::panic::Location<'_>) {
        println!(
            "[State<{}>] [{:?}:{:?}] Try lock -> {}:{}:{}",
            std::any::type_name::<T>(),
            std::thread::current().id(),
            std::ptr::addr_of!(*self),
            caller.file(),
            caller.line(),
            caller.column()
        );
    }

    #[inline(always)]
    fn trace_success(&self, caller: &std::panic::Location<'_>) {
        println!(
            "[State<{}>] [{:?}:{:?}] Locked -> {}:{}:{}",
            std::any::type_name::<T>(),
            std::thread::current().id(),
            std::ptr::addr_of!(*self),
            caller.file(),
            caller.line(),
            caller.column()
        );
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

impl<T: Default + Clone + Send + Sync + std::fmt::Debug> From<T> for State<T> {
    fn from(data: T) -> Self {
        let this = Self::default();
        this.dirty_set(data);
        this
    }
}

impl<T: Clone + Send + Sync + std::fmt::Debug> std::fmt::Debug for State<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", &self.dirty_get())
    }
}

impl<T: Clone + Send + Sync + std::fmt::Display> std::fmt::Display for State<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", &self.dirty_get())
    }
}
