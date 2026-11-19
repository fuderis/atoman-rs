use macron::{Display, Error, From};

#[derive(Debug, Display, Error, From)]
pub enum ConfigError {
    #[from(skip)]
    #[cfg(any(feature = "json-config", feature = "toml-config"))]
    #[display(fmt = "Unsupported config extension `.{0}`.")]
    UnsupportedExt(String),

    #[from(skip)]
    #[cfg(feature = "toml-config")]
    #[display(fmt = "Parse config error: {0}")]
    TomlParseError(Box<toml::de::Error>),

    #[from(skip)]
    #[cfg(feature = "toml-config")]
    #[display(fmt = "Serialize config error: {0}")]
    TomlSerializeError(Box<toml::ser::Error>),

    #[from(skip)]
    #[cfg(feature = "json-config")]
    #[display(fmt = "Parse config error: {0}")]
    JsonParseError(Box<serde_json::Error>),

    #[from(skip)]
    #[cfg(feature = "json-config")]
    #[display(fmt = "Serialize config error: {0}")]
    JsonSerializeError(Box<serde_json::Error>),
}
