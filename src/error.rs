use macron::{Display, Error, From};

#[derive(Debug, Display, Error, From)]
pub enum Error {
    Io(std::io::Error),

    #[cfg(any(feature = "json-config", feature = "toml-config"))]
    Config(crate::config::ConfigError),

    #[cfg(feature = "logger")]
    Logger(crate::logger::LoggerError),

    #[cfg(feature = "channel")]
    Channel(crate::channel::ChannelError),
}
