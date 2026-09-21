#![doc = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/README.md"))]
#![allow(clippy::module_inception)]
pub mod error;
pub mod prelude;

pub mod flag;
pub use flag::Flag;

pub mod state;
pub use state::{State, StateGuard};

pub mod map;
pub use map::{SharedGuard, SharedGuardMut, SharedItem, SharedMap};

pub use arc_swap::{self, ArcSwap, ArcSwapAny};
pub use once_cell::{self, sync::Lazy};

#[cfg(feature = "re-export")]
pub use atoman_macros::{main, test};
#[cfg(feature = "re-export")]
pub use tokio::{
    self, fs, io, net,
    process::{self, Command},
    select,
    sync::{self, Mutex, Notify, RwLock, Semaphore},
    task::{self, JoinHandle, spawn, spawn_blocking},
    time::{self, Duration, Instant, sleep, timeout},
};

#[cfg(any(feature = "json-config", feature = "toml-config"))]
pub mod config;
#[cfg(any(feature = "json-config", feature = "toml-config"))]
pub use config::*;

#[cfg(feature = "logger")]
pub mod logger;
#[cfg(feature = "logger")]
pub use logger::*;

#[cfg(feature = "trace")]
pub mod trace;
#[cfg(feature = "trace")]
pub use trace::*;

#[cfg(feature = "channel")]
pub mod channel;
#[cfg(feature = "channel")]
pub use channel::*;

#[cfg(feature = "file")]
pub mod file;
#[cfg(feature = "file")]
pub use file::*;

/// Initializes static variable by 'once_cell::Lazy'.
#[macro_export]
macro_rules! lazy {
    ($e:expr) => {
        $crate::Lazy::new(|| $e)
    };
}

pub type DynError = Box<dyn std::error::Error + Send + Sync>;
pub type Result<T> = std::result::Result<T, DynError>;
pub use std::result::Result as StdResult;
