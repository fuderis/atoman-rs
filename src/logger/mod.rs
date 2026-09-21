use crate::prelude::*;

pub use atoman_log::log;
pub use tracing::{Instrument, Level, Span, debug, error, info, trace, warn};
use tracing_subscriber::{Layer, layer::Context, registry::LookupSpan};

use bytes::{BufMut, BytesMut};
use chrono::{DateTime, Utc};
use std::{
    path::{Path, PathBuf},
    sync::atomic::{AtomicU8, Ordering},
};
use tokio::{
    fs::{self, File},
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader, BufWriter},
    sync::{mpsc, oneshot},
};

const BUFFER_SIZE: usize = 500_000;
static CURRENT_LEVEL: AtomicU8 = AtomicU8::new(3);
static LOGGER_STATE: State<LoggerState> = State::new(|| Default::default());

pub trait LogExt: Sized {
    fn log_span(self, span: Span) -> tracing::instrument::Instrumented<Self> {
        self.instrument(span)
    }
}

impl<F: std::future::Future> LogExt for F {}

/// Raw event container to bypass string formatting allocations in hot path
pub struct RawLogEvent {
    pub level: Level,
    pub timestamp: DateTime<Utc>,
    pub spans: Option<String>,
    pub message: BytesMut,
}

/// The log command
enum LogCmd {
    Log(RawLogEvent),
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
        LOGGER_STATE.get().path.clone()
    }

    /// Returns the current log level
    pub fn level() -> Level {
        match CURRENT_LEVEL.load(Ordering::Relaxed) {
            1 => Level::ERROR,
            2 => Level::WARN,
            4 => Level::DEBUG,
            5 => Level::TRACE,
            _ => Level::INFO,
        }
    }

    /// Changes the log level
    pub async fn set_level(level: Level) {
        let num = match level {
            Level::ERROR => 1,
            Level::WARN => 2,
            Level::INFO => 3,
            Level::DEBUG => 4,
            Level::TRACE => 5,
        };

        CURRENT_LEVEL.store(num, Ordering::Relaxed);
        LOGGER_STATE.lock().await.level.replace(level);
    }

    /// Initializes the logger
    pub async fn init<P: Into<PathBuf>>(logs_dir: P, max_files: usize) -> Result<()> {
        let logs_dir = logs_dir.into();

        let path = if !logs_dir.as_os_str().is_empty() {
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

            Some(Self::gen_path(logs_dir))
        } else {
            None
        };

        let (tx, rx) = mpsc::channel(BUFFER_SIZE);

        LOGGER_STATE
            .set(LoggerState {
                path,
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
        let tx = LOGGER_STATE.get().tx.clone();
        if let Some(tx) = tx {
            let (tx_signal, rx_signal) = oneshot::channel();
            if tx.send(LogCmd::Flush(tx_signal)).await.is_ok() {
                let _ = rx_signal.await;
            }
        }
    }

    /// Traces all logs in current file by filter
    pub async fn trace(filters: &[String]) -> Result<String> {
        let Some(file_path) = LOGGER_STATE.get().path.clone() else {
            return Err("Log file path is missing".into());
        };

        Self::trace_file(file_path, filters).await
    }

    /// Traces all logs in file by filter
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
        let timestamp = Utc::now();

        // 1. Формируем контекст спанов
        let mut spans_str = None;
        if let Some(scope) = ctx.event_scope(event) {
            let mut span_buf = String::new();
            let mut has_spans = false;

            for span in scope.from_root() {
                if !has_spans {
                    span_buf.push('[');
                    has_spans = true;
                } else {
                    span_buf.push_str(" -> ");
                }
                span_buf.push_str(span.name());

                let extensions = span.extensions();
                if let Some(fields) = extensions.get::<CustomFields>() {
                    span_buf.push('{');
                    span_buf.push_str(&fields.0);
                    span_buf.push('}');
                }
            }
            if has_spans {
                span_buf.push_str("] ");
                spans_str = Some(span_buf);
            }
        }

        // 2. Пишем тело ивента напрямую в байтовый буфер без аллокаций сырых строк
        let mut raw_msg = BytesMut::with_capacity(256);

        struct EventVisitor<'a>(&'a mut BytesMut);
        impl<'a> tracing::field::Visit for EventVisitor<'a> {
            fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
                if field.name() == "message" {
                    let _ = std::fmt::write(self.0, format_args!("{value:?}"));
                } else {
                    let _ = std::fmt::write(self.0, format_args!(" {}={value:?}", field.name()));
                }
            }
        }

        event.record(&mut EventVisitor(&mut raw_msg));

        // 3. Отправляем в воркер сырое событие
        let raw_event = RawLogEvent {
            level,
            timestamp,
            spans: spans_str,
            message: raw_msg,
        };

        self.tx.try_send(LogCmd::Log(raw_event)).ok();
    }
}

/// The log files writer worker
async fn worker(mut rx: mpsc::Receiver<LogCmd>) {
    let mut file = None::<BufWriter<fs::File>>;
    let mut buffer = BytesMut::with_capacity(64 * 1024);
    let mut date = Some(Utc::now().date_naive());
    let mut datetime = String::new();
    let mut timestamp_sec = 0i64;

    while let Some(cmd) = rx.recv().await {
        match cmd {
            LogCmd::Log(raw_event) => {
                let seconds = raw_event.timestamp.timestamp();

                // Кэшируем форматирование даты на уровень воркера
                if seconds != timestamp_sec {
                    datetime = raw_event.timestamp.format("%Y-%m-%dT%H:%M:%SZ").to_string();
                    timestamp_sec = seconds;
                }

                let msg_str = std::str::from_utf8(&raw_event.message).unwrap_or("");

                // Вывод в консоль в режиме отладки
                #[cfg(debug_assertions)]
                {
                    let time_clr = "\x1b[38;5;248m";
                    let meta_clr = "\x1b[34m";
                    let reset = "\x1b[0m";

                    let lvl_clr = match raw_event.level {
                        Level::INFO => "\x1b[32m",
                        Level::WARN => "\x1b[33m",
                        Level::ERROR => "\x1b[31m",
                        Level::DEBUG => "\x1b[35m",
                        Level::TRACE => "\x1b[90m",
                    };

                    if let Some(ref spans) = raw_event.spans {
                        println!(
                            "{time_clr}{datetime}{reset} {lvl_clr}{:<5}{reset} {meta_clr}{spans}{reset}{msg_str}",
                            raw_event.level
                        );
                    } else {
                        println!(
                            "{time_clr}{datetime}{reset} {lvl_clr}{:<5}{reset} {msg_str}",
                            raw_event.level
                        );
                    }
                }

                let path = &LOGGER_STATE.get().path;
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

                let today = raw_event.timestamp.date_naive();
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

                // Записываем собранный логирующий ивент в бинарный буфер файла
                if let Some(writer) = file.as_mut() {
                    buffer.put_slice(datetime.as_bytes());
                    buffer.put_u8(b' ');
                    let lvl_str = format!("{:<5}", raw_event.level);
                    buffer.put_slice(lvl_str.as_bytes());
                    buffer.put_u8(b' ');

                    if let Some(ref spans) = raw_event.spans {
                        buffer.put_slice(spans.as_bytes());
                    }

                    buffer.put_slice(&raw_event.message);
                    buffer.put_u8(b'\n');

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
