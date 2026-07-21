#![allow(unused_imports)]
use crate::prelude::DynError;
use macron::{Display, Error, From};

// The error
#[derive(Debug, Display, Error, From)]
pub enum Error {
    #[from]
    Io(std::io::Error),

    #[cfg(any(feature = "json-config", feature = "toml-config"))]
    #[display = "Unsupported config extension '.{0}'."]
    ConfigExt(String),

    #[cfg(any(feature = "json-config", feature = "toml-config"))]
    #[display = "Parse config error: {0}"]
    ParseConfig(DynError),

    #[cfg(feature = "trace")]
    #[display = "Failed to open file: {0}"]
    OpenFile(std::io::Error),

    #[cfg(feature = "trace")]
    #[display = "Failed to read file: {0}"]
    ReadFile(std::io::Error),

    #[cfg(feature = "channel")]
    #[display = "Channel is already closed"]
    ChannelClosed,

    #[cfg(feature = "channel")]
    #[display = "Channel is overflowed"]
    ChannelFull,
}
