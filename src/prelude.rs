#![allow(unused_imports)]
pub(crate) use error::Error;
pub(crate) use macron::*;
pub(crate) use std::fmt::Debug as Debugging;
pub(crate) use std::fmt::Display as Displaying;
pub(crate) use std::path::{Path, PathBuf};
pub(crate) use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex, MutexGuard,
};
pub(crate) use tokio::sync::Notify;

/// The dynamic error type
#[allow(dead_code)]
pub(crate) type DynError = Box<dyn std::error::Error + Send + Sync + 'static>;
/// The short result alias
#[allow(dead_code)]
pub(crate) type Result<T> = std::result::Result<T, DynError>;
/// The std result alias
pub(crate) use std::result::Result as StdResult;

pub use crate::*;
