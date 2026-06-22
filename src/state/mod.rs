pub mod guard;
pub use guard::StateGuard;

pub(super) const ERR_MSG: &str = "The data has been poisoned!";

use crate::flag::Flag;
use crate::prelude::*;
use once_cell::sync::OnceCell;

/// The atomic state wrapper
#[derive(Clone)]
pub struct StateWrap<T: Clone + Send + Sync> {
    mutex: Arc<Mutex<Arc<T>>>,
    swap: Arc<ArcSwapAny<Arc<T>>>,
    lock: Arc<Flag>,
}

/// The atomic state
pub struct State<T: Clone + Send + Sync + 'static> {
    wrap: OnceCell<Arc<StateWrap<T>>>,
    init_fn: fn() -> T,
}

impl<T: Clone + Send + Sync> State<T> {
    /// Creates a new state with custom initializator
    pub const fn new(init_fn: fn() -> T) -> Self {
        Self {
            wrap: OnceCell::new(),
            init_fn,
        }
    }

    /// An internal method for lazy initialization on first access
    fn get_or_init(&self) -> &Arc<StateWrap<T>> {
        self.wrap.get_or_init(|| {
            let initial_value = (self.init_fn)();
            let arc_val = Arc::new(initial_value);

            Arc::new(StateWrap {
                mutex: Arc::new(Mutex::new(arc_val.clone())),
                swap: Arc::new(ArcSwapAny::from(arc_val)),
                lock: Arc::new(Flag::new()),
            })
        })
    }

    /// Returns true if data locked by some StateGuard
    pub fn is_locked(&self) -> bool {
        self.get_or_init().lock.is_locked()
    }

    /// Returns a state guard
    #[track_caller]
    pub fn lock(&self) -> impl std::future::Future<Output = StateGuard<T>> + '_ {
        #[cfg(feature = "trace-lock")]
        let caller = std::panic::Location::caller();
        #[cfg(feature = "trace-lock")]
        self.trace_attempt(caller);

        async move {
            let wrap = self.get_or_init();
            wrap.lock.lock().await;

            #[cfg(feature = "trace-lock")]
            self.trace_success(caller);

            StateGuard {
                mutex: wrap.mutex.clone(),
                swap: wrap.swap.clone(),
                data: self.dirty_get_cloned(),
                lock: wrap.lock.clone(),
                counter: 0,
            }
        }
    }

    /// Returns a state guard (with synchronously blocking)
    #[track_caller]
    pub fn blocking_lock(&self) -> StateGuard<T> {
        #[cfg(feature = "trace-lock")]
        let caller = std::panic::Location::caller();
        #[cfg(feature = "trace-lock")]
        self.trace_attempt(caller);

        let wrap = self.get_or_init();
        wrap.lock.blocking_lock();

        #[cfg(feature = "trace-lock")]
        self.trace_success(caller);

        StateGuard {
            mutex: wrap.mutex.clone(),
            swap: wrap.swap.clone(),
            data: self.dirty_get_cloned(),
            lock: wrap.lock.clone(),
            counter: 0,
        }
    }

    /// Returns a state guard (warning: changes not be saved if one of StateGuard is alive)
    #[track_caller]
    pub fn dirty_lock(&self) -> StateGuard<T> {
        let wrap = self.get_or_init();

        if !wrap.lock.is_locked() {
            let _ = wrap.lock.try_lock();
        }

        StateGuard {
            mutex: wrap.mutex.clone(),
            swap: wrap.swap.clone(),
            data: self.dirty_get_cloned(),
            lock: wrap.lock.clone(),
            counter: 0,
        }
    }

    /// Returns a state value
    pub async fn get(&self) -> Arc<T> {
        let wrap = self.get_or_init();
        if wrap.lock.is_locked() {
            wrap.lock.lock().await;
            wrap.lock.unlock();
        }
        self.dirty_get()
    }

    /// Returns a state value (with synchronously blocking)
    pub fn blocking_get(&self) -> Arc<T> {
        let wrap = self.get_or_init();
        if wrap.lock.is_locked() {
            wrap.lock.blocking_lock();
            wrap.lock.unlock();
        }
        self.dirty_get()
    }

    /// Returns a state value (warning: may not contain actual data)
    pub fn dirty_get(&self) -> Arc<T> {
        self.get_or_init().swap.load_full()
    }

    /// Returns a clone of state value
    pub async fn get_cloned(&self) -> T {
        let wrap = self.get_or_init();
        if wrap.lock.is_locked() {
            wrap.lock.lock().await;
            wrap.lock.unlock();
        }
        self.dirty_get_cloned()
    }

    /// Returns a clone of state value (with synchronously blocking)
    pub fn blocking_get_cloned(&self) -> T {
        let wrap = self.get_or_init();
        if wrap.lock.is_locked() {
            wrap.lock.blocking_lock();
            wrap.lock.unlock();
        }
        self.dirty_get_cloned()
    }

    /// Returns a clone of state value (warning: may not contain actual data)
    pub fn dirty_get_cloned(&self) -> T {
        self.get_or_init().swap.load_full().as_ref().clone()
    }

    /// Sets a new value to state
    pub async fn set(&self, value: T) {
        let wrap = self.get_or_init();
        wrap.lock.lock().await;

        self.dirty_set(value);
        wrap.lock.unlock();
    }

    /// Sets a new value to state (with synchronously blocking)
    pub fn blocking_set(&self, value: T) {
        let wrap = self.get_or_init();
        wrap.lock.blocking_lock();

        self.dirty_set(value);
        wrap.lock.unlock();
    }

    /// Sets a new value to state (warning: changes not be saved if one of StateGuard is alive)
    pub fn dirty_set(&self, value: T) {
        let new_data = Arc::new(value);
        let mut lock = self.get_or_init().mutex.lock().expect(ERR_MSG);
        *lock = new_data.clone();
        self.get_or_init().swap.store(new_data);
    }

    //      LOCK TRACING

    #[inline(always)]
    #[cfg(feature = "trace-lock")]
    fn trace_attempt(&self, caller: &std::panic::Location<'_>) {
        crate::trace!(
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
    #[cfg(feature = "trace-lock")]
    fn trace_success(&self, caller: &std::panic::Location<'_>) {
        crate::trace!(
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
    /// Creates a new state with default value
    pub const fn default() -> Self {
        Self {
            wrap: OnceCell::new(),
            init_fn: T::default,
        }
    }
}

impl<T: Default + Clone + Send + Sync> ::std::default::Default for State<T> {
    fn default() -> Self {
        Self::default()
    }
}

impl<T: Default + Clone + Send + Sync + Debugging> From<T> for State<T> {
    fn from(data: T) -> Self {
        let this = Self::default();
        this.dirty_set(data);
        this
    }
}

impl<T: Clone + Send + Sync + Debugging> ::std::fmt::Debug for State<T> {
    fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
        write!(f, "{:?}", &self.dirty_get())
    }
}

impl<T: Clone + Send + Sync + Displaying> ::std::fmt::Display for State<T> {
    fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
        write!(f, "{}", &self.dirty_get())
    }
}
