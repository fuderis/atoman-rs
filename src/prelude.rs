#![allow(unused_imports)]
pub use crate::{State, error::Error};

pub use macron::*;
pub use std::path::{Path, PathBuf};
pub use std::sync::{
    Arc, MutexGuard,
    atomic::{AtomicBool, Ordering},
};

#[allow(dead_code)]
pub type DynError = Box<dyn std::error::Error + Send + Sync>;
#[allow(dead_code)]
pub type Result<T> = std::result::Result<T, DynError>;
pub use std::result::Result as StdResult;
