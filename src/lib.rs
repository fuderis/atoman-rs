#![doc = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/README.md"))]
#![allow(clippy::module_inception)]
pub mod error;
pub mod prelude;

pub mod flag;
pub use flag::Flag;

pub mod state;
pub use state::{State, StateGuard};

pub mod map;
pub use map::{Map, MapGuard, MapGuardMut};

pub use arc_swap::{self, ArcSwap, ArcSwapAny};
pub use once_cell::{self, sync::Lazy};

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

/// Initializes a static variable by 'once_cell::Lazy'
#[macro_export]
macro_rules! lazy {
    ($e:expr) => {
        $crate::Lazy::new(|| $e)
    };
}
