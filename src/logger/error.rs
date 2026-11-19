use macron::{Display, Error, From};

#[derive(Debug, Display, Error, From)]
pub enum LoggerError {
    #[from(skip)]
    #[display(fmt = "Log file path is missing.")]
    FilePathMissing,
}
