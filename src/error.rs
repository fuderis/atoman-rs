#![allow(unused_imports)]
use crate::prelude::DynError;
use macron::{Display, Error, From};

// The error
#[derive(Debug, Display, Error, From)]
pub enum Error {
    Io(std::io::Error),

    #[cfg(any(feature = "json-config", feature = "toml-config"))]
    #[display(fmt = "Unsupported config extension '.{0}'.")]
    ConfigExt(String),

    #[cfg(any(feature = "json-config", feature = "toml-config"))]
    #[display(fmt = "Parse config error: {0}")]
    ParseConfig(DynError),

    #[from(skip)]
    #[cfg(feature = "trace")]
    #[display(fmt = "Failed to open file: {0}")]
    OpenFile(std::io::Error),

    #[from(skip)]
    #[cfg(feature = "trace")]
    #[display(fmt = "Failed to read file: {0}")]
    ReadFile(std::io::Error),

    #[cfg(feature = "channel")]
    #[display(fmt = "Channel is already closed")]
    ChannelClosed,

    #[cfg(feature = "channel")]
    #[display(fmt = "Channel is overflowed")]
    ChannelFull,
}
