use macron::{Display, Error, From};

#[derive(Debug, Display, Error, From)]
pub enum Error {
    IoError(std::io::Error),

    #[cfg(any(feature = "json-config", feature = "toml-config"))]
    ConfigError(crate::config::ConfigError),

    #[cfg(feature = "logger")]
    LoggerError(crate::logger::LoggerError),

    #[cfg(feature = "channel")]
    ChannelError(crate::channel::ChannelError),
}
