use crate::prelude::*;

pub use tracing::{Instrument, Level, Span, debug, error, info, instrument as log, trace, warn};
use tracing_subscriber::{Layer, layer::Context, registry::LookupSpan};

use bytes::{BufMut, BytesMut};
use chrono::Utc;
use std::path::{Path, PathBuf};
use tokio::{
    fs::{self, File},
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader, BufWriter},
    sync::{mpsc, oneshot},
};

const BUFFER_SIZE: usize = 500_000;
static LOGGER_STATE: State<LoggerState> = State::new();

/// The log command
enum LogCmd {
    Log(Level, String),
    Flush(oneshot::Sender<()>),
}

/// The custom fields wrapper
struct CustomFields(String);

/// The logger state
#[derive(Default, Clone)]
struct LoggerState {
    level: Option<Level>,
    path: Option<PathBuf>,
    tx: Option<mpsc::Sender<LogCmd>>,
}

/// The atomic logger
pub struct Logger {
    tx: mpsc::Sender<LogCmd>,
}

impl Logger {
    /// Returns the current .log file path
    pub fn path() -> Option<PathBuf> {
        LOGGER_STATE.dirty_get().path.clone()
    }

    /// Returns the current log level
    pub fn level() -> Level {
        LOGGER_STATE
            .dirty_get()
            .level
            .clone()
            .unwrap_or(Level::INFO)
    }

    /// Changes the log level
    pub async fn set_level(level: Level) {
        LOGGER_STATE.lock().await.level.replace(level);
    }

    /// Initializes the logger
    pub async fn init<P: Into<PathBuf>>(logs_dir: P, max_files: usize) -> Result<()> {
        let logs_dir = logs_dir.into();
        fs::create_dir_all(&logs_dir).await?;

        if max_files > 0 {
            let mut entries = fs::read_dir(&logs_dir).await?;
            let mut files = vec![];
            while let Some(entry) = entries.next_entry().await? {
                let path = entry.path();
                if path.extension().map_or(false, |ext| ext == "log") {
                    files.push((path, entry.metadata().await.and_then(|m| m.created()).ok()));
                }
            }
            files.sort_by_key(|(_, time)| *time);
            if files.len() > max_files {
                for (old_file, _) in &files[0..files.len() - max_files] {
                    let _ = fs::remove_file(old_file).await;
                }
            }
        }

        let path = Self::gen_path(logs_dir);
        let (tx, rx) = mpsc::channel(BUFFER_SIZE);

        LOGGER_STATE
            .set(LoggerState {
                path: Some(path),
                tx: Some(tx.clone()),
                level: None,
            })
            .await;

        tokio::spawn(async move {
            worker(rx).await;
        });

        let logger = Self { tx };

        use tracing_subscriber::prelude::*;
        tracing_subscriber::registry().with(logger).init();

        Ok(())
    }

    /// Forced push of the worker buffer to disk
    pub async fn flush() {
        let tx = LOGGER_STATE.dirty_get().tx.clone();
        if let Some(tx) = tx {
            let (tx_signal, rx_signal) = oneshot::channel();
            if tx.send(LogCmd::Flush(tx_signal)).await.is_ok() {
                let _ = rx_signal.await;
            }
        }
    }

    /// Traces the all logs in current file by filter
    pub async fn trace(filters: &[String]) -> Result<String> {
        let Some(file_path) = LOGGER_STATE.dirty_get().path.clone() else {
            return Err("Log file path is missing".into());
        };

        Self::trace_file(file_path, filters).await
    }

    /// Traces the all logs in file by filter
    pub async fn trace_file(file_path: impl AsRef<Path>, filters: &[String]) -> Result<String> {
        Self::flush().await;

        let file = File::open(file_path.as_ref()).await?;
        let mut reader = BufReader::new(file).lines();
        let mut matched_lines = String::new();

        while let Some(line) = reader.next_line().await? {
            let is_match = filters.iter().all(|filter| line.contains(filter));

            if is_match {
                matched_lines.push_str(&line);
                matched_lines.push('\n');
            }
        }

        Ok(matched_lines)
    }

    /// Creates a new log file path
    pub fn gen_path(dir: impl AsRef<Path>) -> PathBuf {
        let dt = Utc::now().format("%Y-%m-%d_%H-%M-%S%.6f").to_string();
        let pid = std::process::id();
        dir.as_ref().join(format!("{dt}_{pid}.log"))
    }
}

impl<S> Layer<S> for Logger
where
    S: tracing::Subscriber + for<'a> LookupSpan<'a>,
{
    fn enabled(&self, metadata: &tracing::Metadata<'_>, _ctx: Context<'_, S>) -> bool {
        metadata.level() <= &Self::level()
    }

    fn on_new_span(
        &self,
        attrs: &tracing::span::Attributes<'_>,
        id: &tracing::Id,
        ctx: Context<'_, S>,
    ) {
        let span = ctx.span(id).expect("Спан обязан существовать");
        let mut fields_str = String::new();

        struct SpanFieldsVisitor<'a>(&'a mut String, bool);
        impl<'a> tracing::field::Visit for SpanFieldsVisitor<'a> {
            fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
                if !self.1 {
                    self.0.push_str(", ");
                }
                let _ = std::fmt::write(self.0, format_args!("{}={value:?}", field.name()));
                self.1 = false;
            }
        }

        let mut visitor = SpanFieldsVisitor(&mut fields_str, true);
        attrs.record(&mut visitor);

        if !fields_str.is_empty() {
            let mut extensions = span.extensions_mut();
            extensions.insert(CustomFields(fields_str));
        }
    }

    fn on_event(&self, event: &tracing::Event<'_>, ctx: Context<'_, S>) {
        let level = *event.metadata().level();
        let mut msg = String::new();

        let mut has_spans = false;
        if let Some(scope) = ctx.event_scope(event) {
            for span in scope.from_root() {
                if !has_spans {
                    msg.push('[');
                    has_spans = true;
                } else {
                    msg.push_str(" -> ");
                }
                msg.push_str(span.name());

                let extensions = span.extensions();
                if let Some(fields) = extensions.get::<CustomFields>() {
                    msg.push('{');
                    msg.push_str(&fields.0);
                    msg.push('}');
                }
            }
            if has_spans {
                msg.push_str("] ");
            }
        }

        struct StringVisitor<'a>(&'a mut String);
        impl<'a> tracing::field::Visit for StringVisitor<'a> {
            fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
                if field.name() == "message" {
                    let _ = std::fmt::write(self.0, format_args!("{value:?}"));
                } else {
                    let _ = std::fmt::write(self.0, format_args!(" {}={value:?}", field.name()));
                }
            }
        }

        event.record(&mut StringVisitor(&mut msg));
        self.tx.try_send(LogCmd::Log(level, msg)).ok();
    }
}

/// The log files writer
async fn worker(mut rx: mpsc::Receiver<LogCmd>) {
    let mut file = None::<BufWriter<fs::File>>;
    let mut buffer = BytesMut::with_capacity(64 * 1024);
    let mut date = Some(Utc::now().date_naive());
    let mut datetime = String::new();
    let mut timestamp = 0i64;

    while let Some(cmd) = rx.recv().await {
        match cmd {
            LogCmd::Log(lvl, msg) => {
                let now = Utc::now();
                let seconds = now.timestamp();

                if seconds != timestamp {
                    datetime = now.format("%Y-%m-%dT%H:%M:%SZ").to_string();
                    timestamp = seconds;
                }

                #[cfg(debug_assertions)]
                {
                    let time_clr = "\x1b[38m";
                    let meta_clr = "\x1b[34m";
                    let reset = "\x1b[0m";

                    let lvl_clr = match lvl {
                        Level::INFO => "\x1b[32m",
                        Level::WARN => "\x1b[33m",
                        Level::ERROR => "\x1b[31m",
                        Level::DEBUG => "\x1b[35m",
                        Level::TRACE => "\x1b[90m",
                    };

                    if msg.starts_with('[') {
                        if let Some(pos) = msg.find("] ") {
                            let meta = &msg[..=pos];
                            let body = &msg[pos + 1..];

                            println!(
                                "{time_clr}{datetime}{reset} {lvl_clr}{lvl:<5}{reset} {meta_clr}{meta}{reset}{body}"
                            );
                        } else {
                            println!("{meta_clr}{datetime}{reset} {lvl_clr}{lvl:<5}{reset} {msg}");
                        }
                    } else {
                        println!("{meta_clr}{datetime}{reset} {lvl_clr}{lvl:<5}{reset} {msg}");
                    }
                }

                let path = &LOGGER_STATE.dirty_get().path;
                let Some(current_path) = path else {
                    continue;
                };

                if file.is_none() {
                    if let Ok(f) = fs::OpenOptions::new()
                        .create(true)
                        .append(true)
                        .open(current_path)
                        .await
                    {
                        file = Some(BufWriter::with_capacity(128 * 1024, f));
                    }
                }

                let today = now.date_naive();
                if date != Some(today) {
                    if let Some(mut old_writer) = file.take() {
                        if !buffer.is_empty() {
                            let _ = old_writer.write_all(&buffer).await;
                            buffer.clear();
                        }
                        let _ = old_writer.flush().await;
                    }

                    if let Some(parent_dir) = current_path.parent() {
                        let new_path = Logger::gen_path(parent_dir);
                        if let Ok(f) = fs::OpenOptions::new()
                            .create(true)
                            .append(true)
                            .open(&new_path)
                            .await
                        {
                            file = Some(BufWriter::with_capacity(128 * 1024, f));
                            date = Some(today);
                        }
                        LOGGER_STATE.lock().await.path.replace(new_path);
                    }
                }

                if let Some(writer) = file.as_mut() {
                    let line = format!("{datetime} {lvl:<5} {msg}\n");
                    buffer.put_slice(line.as_bytes());

                    if rx.is_empty() || buffer.len() > 48 * 1024 {
                        if writer.write_all(&buffer).await.is_ok() {
                            let _ = writer.flush().await;
                        }
                        buffer.clear();
                    }
                }
            }

            LogCmd::Flush(res_tx) => {
                if let Some(writer) = file.as_mut() {
                    if !buffer.is_empty() {
                        if writer.write_all(&buffer).await.is_ok() {
                            let _ = writer.flush().await;
                        }
                        buffer.clear();
                    } else {
                        let _ = writer.flush().await;
                    }
                }
                let _ = res_tx.send(());
            }
        }
    }
}
