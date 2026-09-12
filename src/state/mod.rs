pub mod guard;
pub use guard::StateGuard;

use arc_swap::ArcSwapAny;
use once_cell::sync::OnceCell;
use std::sync::Arc;
use tokio::sync::Mutex;

/// Internal state wrapper.
///
/// `Mutex<Arc<T>>` protects the modification process,
/// while `ArcSwapAny` provides instant lock‑free reading (lock‑free read).
#[derive(Clone)]
pub struct StateWrap<T: Clone + Send + Sync> {
    mutex: Arc<Mutex<()>>,
    swap: Arc<ArcSwapAny<Arc<T>>>,
}

/// Atomic shared state with lock-free reading via `ArcSwap`.
pub struct State<T: Clone + Send + Sync + 'static> {
    wrap: OnceCell<StateWrap<T>>,
    init_fn: fn() -> T,
}

impl<T: Clone + Send + Sync> State<T> {
    /// Lazy initialization on first access.
    fn get_or_init(&self) -> &StateWrap<T> {
        self.wrap.get_or_init(|| {
            let value = Arc::new((self.init_fn)());
            StateWrap {
                mutex: Arc::new(Mutex::new(())),
                swap: Arc::new(ArcSwapAny::from(value)),
            }
        })
    }
}

impl<T: Clone + Send + Sync> State<T> {
    /// Creates new state with custom initializator.
    pub const fn new(init_fn: fn() -> T) -> Self {
        Self {
            wrap: OnceCell::new(),
            init_fn,
        }
    }

    /// Returns state guard asynchronously.
    pub async fn lock(&self) -> StateGuard<T> {
        let wrap = self.get_or_init().clone();

        StateGuard {
            _guard: wrap.mutex.lock_owned().await,
            swap: wrap.swap.clone(),
            data: (*wrap.swap.load_full()).clone(),
            counter: 0,
        }
    }

    /// Sets new value to state asynchronously.
    pub async fn set(&self, value: T) {
        let wrap = self.get_or_init();
        let _ = wrap.mutex.lock().await;
        wrap.swap.store(Arc::new(value));
    }

    /// Sets new value to state synchronously.
    pub fn blocking_set(&self, value: T) {
        let wrap = self.get_or_init();
        let _ = wrap.mutex.blocking_lock();
        wrap.swap.store(Arc::new(value));
    }

    /// Returns state value instantly without checking locks (Lock-free).
    #[inline]
    pub fn get(&self) -> Arc<T> {
        self.get_or_init().swap.load_full()
    }

    /// Returns clone of state value instantly without locks.
    #[inline]
    pub fn get_cloned(&self) -> T {
        self.get().as_ref().clone()
    }
}

impl<T: Clone + Send + Sync + 'static> Clone for State<T> {
    fn clone(&self) -> Self {
        let wrap = self.get_or_init();

        let new_wrap = OnceCell::new();
        let _ = new_wrap.set(wrap.clone());

        Self {
            wrap: new_wrap,
            init_fn: self.init_fn,
        }
    }
}

impl<T: Default + Clone + Send + Sync + 'static> State<T> {
    pub const fn default() -> Self {
        Self::new(T::default)
    }
}

impl<T: Default + Clone + Send + Sync> Default for State<T> {
    fn default() -> Self {
        Self::new(T::default)
    }
}

impl<T: Clone + Send + Sync + 'static> From<T> for State<T> {
    fn from(value: T) -> Self {
        let wrap = StateWrap {
            mutex: Arc::new(Mutex::new(())),
            swap: Arc::new(ArcSwapAny::from(Arc::new(value))),
        };

        let once = OnceCell::new();
        let _ = once.set(wrap);

        Self {
            wrap: once,
            init_fn: || unreachable!("State initialised via From<T>"),
        }
    }
}

impl<T: Clone + Send + Sync + std::fmt::Debug> std::fmt::Debug for State<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", &self.get())
    }
}

impl<T: Clone + Send + Sync + std::fmt::Display> std::fmt::Display for State<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", &self.get())
    }
}
