use crate::prelude::*;

pub use log::{self, Level, debug, error, info, trace, warn};

use bytes::{BufMut, BytesMut};
use chrono::{NaiveDate, Utc};
use std::path::PathBuf;
use tokio::fs;
use tokio::io::{AsyncWriteExt, BufWriter};
use tokio::sync::mpsc;

/// The limited channel to protect memory
const BUFFER_SIZE: usize = 500_000;

/// The logger global instance
static LOGGER: Lazy<Logger> = Lazy::new(|| Logger::new());

/// The logger payload data
type LogPayload = (Level, String);

/// The logger
pub struct Logger {
    pub(super) level: State<Option<Level>>,
    pub(super) path: State<Option<PathBuf>>,
    pub(super) tx: Arc<mpsc::Sender<LogPayload>>,
}

impl Logger {
    /// Creates a new instance of logger
    fn new() -> Self {
        let (tx, rx) = mpsc::channel(BUFFER_SIZE);

        tokio::spawn(async move {
            worker(rx).await;
        });

        Self {
            level: State::from(Some(Level::Info)),
            path: State::from(None),
            tx: arc!(tx),
        }
    }

    /// Returns the current .log file path
    pub fn path() -> Option<PathBuf> {
        LOGGER.path.dirty_get_cloned()
    }

    /// Returns log level
    pub fn level() -> Level {
        LOGGER.level.dirty_get_cloned().unwrap_or(Level::Info)
    }

    /// Sets minimum log level
    pub async fn set_level(level: Level) {
        LOGGER.level.set(Some(level)).await;
    }

    /// Initializes logger
    pub async fn init<P: Into<PathBuf>>(logs_dir: P, max_files: usize) -> Result<()> {
        let logs_dir = logs_dir.into();

        // create logs dir:
        fs::create_dir_all(&logs_dir).await?;

        // remove extra files by limit:
        if max_files > 0 {
            // read logs dir:
            let mut entries = fs::read_dir(&logs_dir).await?;
            let mut files = vec![];
            while let Some(entry) = entries.next_entry().await? {
                let path = entry.path();

                if path.extension().map_or(false, |ext| ext == "log") {
                    files.push((path, entry.metadata().await.and_then(|m| m.created()).ok()));
                }
            }

            // sort log files by time:
            files.sort_by_key(|(_, time)| *time);

            // remove extra files:
            if files.len() > max_files {
                for (old_file, _) in &files[0..files.len() - max_files] {
                    let _ = fs::remove_file(old_file).await?;
                }
            }
        }

        LOGGER.path.dirty_set(Some(Self::new_path(logs_dir)));
        LOGGER.init_self()
    }

    /// Creates a new log-file path
    fn new_path(dir: impl AsRef<Path>) -> PathBuf {
        let dt = Utc::now().format("%Y-%m-%d_%H-%M-%S").to_string();
        let pid = std::process::id();

        dir.as_ref().join(format!("{dt}_{pid}.log"))
    }

    /// Helper method to initialize logger
    fn init_self(&'static self) -> Result<()> {
        log::set_logger(self).map_err(|e| Error::from(e))?;
        log::set_max_level(log::LevelFilter::Info);
        Ok(())
    }
}

impl log::Log for Logger {
    fn enabled(&self, metadata: &log::Metadata) -> bool {
        metadata.level() <= Self::level()
    }

    fn log(&self, record: &log::Record) {
        if !self.enabled(record.metadata()) {
            return;
        }

        // convert arguments into a string:
        let msg = record.args().to_string();
        let level = record.level();

        // send log into worker (drop on buffer overflow to avoid OOM):
        self.tx.try_send((level, msg)).ok();
    }

    fn flush(&self) {}
}

/// Asynchronous worker that writes logs to the file
async fn worker(mut rx: mpsc::Receiver<LogPayload>) {
    let mut file = None::<BufWriter<fs::File>>;
    let mut buffer = BytesMut::with_capacity(64 * 1024);

    let mut date: Option<NaiveDate> = None;
    let mut datetime = String::new();
    let mut timestamp = 0i64;

    while let Some((lvl, msg)) = rx.recv().await {
        let now = Utc::now();
        let seconds = now.timestamp();

        // update datetime string only if a second has passed:
        if seconds != timestamp {
            datetime = now.format("%Y-%m-%dT%H:%M:%SZ").to_string();
            timestamp = seconds;
        }

        // output to console is for debug only:
        #[cfg(debug_assertions)]
        {
            let clr = match lvl {
                log::Level::Info => "\x1b[32m",  // green
                log::Level::Warn => "\x1b[33m",  // yellow
                log::Level::Error => "\x1b[31m", // red
                log::Level::Debug => "\x1b[34m", // blue
                log::Level::Trace => "\x1b[36m", // cyan
            };
            println!("{datetime} {clr}{lvl:<5}\x1b[0m {msg}");
        }

        // if file path is not set, continue:
        let path = LOGGER.path.dirty_get();
        if path.is_none() {
            continue;
        }

        // check if file needs to be changed (rotation):
        let today = now.date_naive();
        if date != Some(today) {
            // discard remnants of the old file:
            if let Some(mut old_writer) = file.take() {
                if !buffer.is_empty() {
                    old_writer.write_all(&buffer).await.ok();
                    buffer.clear();
                }
                old_writer.flush().await.ok();
            }

            // unwrap logs directory from path:
            if let Some(path) = path.as_ref()
                && let Some(parent_dir) = path.parent()
            {
                let new_path = Logger::new_path(parent_dir);

                if let Ok(f) = fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(&new_path)
                    .await
                {
                    file = Some(BufWriter::with_capacity(128 * 1024, f));
                    date = Some(today);
                }

                // update path in state:
                LOGGER.path.set(Some(new_path)).await;
            }
        }

        // get file writer (if exists):
        if let Some(writer) = file.as_mut() {
            // write log to the buffer:
            let line = format!("{datetime} {lvl:<5} {msg}\n");
            buffer.put_slice(line.as_bytes());

            // flush buffer if queue is empty OR has reached the buffer limit:
            if rx.is_empty() || buffer.len() > 48 * 1024 {
                if writer.write_all(&buffer).await.is_ok() {
                    let _ = writer.flush().await;
                }
                buffer.clear();
            }
        }
    }
}
