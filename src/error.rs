use macron::{Display, Error, From};

// The error
#[derive(Debug, Display, Error, From)]
pub enum Error {
    #[from]
    Io(std::io::Error),

    #[cfg(any(feature = "json-config", feature = "toml-config"))]
    #[display = "Unsupported config extension '.{0}'."]
    ConfigExt(String),

    #[cfg(feature = "trace")]
    #[display = "Failed to open file: {0}"]
    OpenFile(std::io::Error),

    #[cfg(feature = "trace")]
    #[display = "Failed to read file: {0}"]
    ReadFile(std::io::Error),

    #[cfg(feature = "stream")]
    #[display = "Unexpected EOF with partial data"]
    UnexpectedEOF,

    #[cfg(feature = "stream")]
    #[display = "Stream channel is already closed"]
    StreamClosed,
}
