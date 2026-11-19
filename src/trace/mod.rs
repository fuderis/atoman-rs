use crate::{State, prelude::*};

use regex::Regex;
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{Arc, OnceLock},
    time::Duration,
};
use tokio::{
    fs::{self as tfs, File},
    io::{AsyncBufReadExt, AsyncSeekExt, BufReader, SeekFrom},
    sync::{RwLock, mpsc},
    task::JoinHandle,
    time::sleep,
};

const ANSI_RESET: &str = "\x1b[0m";
const ANSI_BOLD: &str = "\x1b[1m";
const ANSI_DIM: &str = "\x1b[2m";
const ANSI_TIME: &str = "\x1b[38;5;248m";
const ANSI_RED: &str = "\x1b[31m";
const ANSI_GREEN: &str = "\x1b[32m";
const ANSI_YELLOW: &str = "\x1b[33m";
const ANSI_BLUE: &str = "\x1b[34m";
const ANSI_MAGENTA: &str = "\x1b[35m";
const ANSI_CYAN: &str = "\x1b[36m";
const ANSI_GRAY: &str = "\x1b[90m";

/// Main header regular expression compiled once via `State`.
///
/// Also acts as the entry boundary detector: lines matching this header start a new `Log`,
/// while subsequent non-matching lines are appended as multiline continuation.
static HEADER_REGEX: State<Regex> = State::new(|| {
    Regex::new(
        r"(?x)
        ^(?P<time>\d{4}-\d{2}-\d{2}[T\s]\d{2}:\d{2}:\d{2}(?:\.\d+)?(?:Z|[+-]\d{2}:?\d{2})?)\s+
        (?P<level>TRACE|DEBUG|INFO|WARN|WARNING|ERROR|ERR|FATAL)
        (?P<meta>(?:\s*\[[^\]]+\])*)
        (?:\s+(?P<msg>[^\r\n]*))?
        ",
    )
    .expect("valid log header regex")
});

static BRACKET_BLOCK_RE: State<Regex> =
    State::new(|| Regex::new(r"\[([^\]]+)\]").expect("valid bracket block regex"));

static SPAN_BLOCK_RE: State<Regex> =
    State::new(|| Regex::new(r"^([A-Za-z0-9_:-]+)\{([^}]*)\}$").expect("valid span block regex"));

/// Parsed span metadata extracted from a bracketed block like `[login{uid=1}]`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct LogSpan {
    /// Span name (e.g. `login` or `handle`).
    pub span: String,
    /// Key-value parameters attached to the span.
    pub params: HashMap<String, String>,
}

/// Parsed log entry with ANSI-colored `Display` representation.
#[derive(Debug, Display, Clone)]
#[display(
    fmt = "{ANSI_BOLD}[{source}]{ANSI_RESET}{ts}{lvl}{span_part}{sub}{msg}",
    ts = self.format_timestamp(),
    lvl = self.format_level(),
    span_part = self.format_span(),
    sub = self.format_sublevel(),
    msg = self.format_message(),
)]
pub struct Log {
    /// Name or identifier of the log source.
    pub source: String,
    /// Timestamp extracted from the log entry header, if present.
    pub timestamp: Option<String>,
    /// Log severity level (e.g. INFO, WARN, ERROR).
    pub level: Option<String>,
    /// Optional span with its name and parameters.
    pub span: Option<LogSpan>,
    /// Optional sublevel / component / module tag.
    pub sublevel: Option<String>,
    /// Log message payload (including multiline continuation lines).
    pub message: String,
    /// Extracted key-value parameters from the log entry for easy filtering/inspection.
    pub parameters: HashMap<String, String>,
}

impl Log {
    /// Checks whether all given `(key, value)` pairs match parsed parameters.
    pub fn has_params(&self, expected: &[(String, String)]) -> bool {
        expected.iter().all(|(k, v)| {
            self.parameters.get(k).is_some_and(|actual| actual == v)
                || self
                    .span
                    .as_ref()
                    .and_then(|s| s.params.get(k))
                    .is_some_and(|actual| actual == v)
        })
    }

    fn format_timestamp(&self) -> String {
        match &self.timestamp {
            Some(ts) => format!(" {ANSI_TIME}{ts}{ANSI_RESET}"),
            None => String::new(),
        }
    }

    fn format_level(&self) -> String {
        let Some(lvl) = &self.level else {
            return String::new();
        };

        let clr = match lvl.as_str() {
            "ERROR" | "FATAL" => ANSI_RED,
            "WARN" => ANSI_YELLOW,
            "INFO" => ANSI_GREEN,
            "DEBUG" => ANSI_BLUE,
            "TRACE" => ANSI_GRAY,
            _ => "",
        };

        if clr.is_empty() {
            format!(" {lvl}")
        } else {
            format!(" {clr}{lvl}{ANSI_RESET}")
        }
    }

    fn format_span(&self) -> String {
        let Some(span) = &self.span else {
            return String::new();
        };

        if span.params.is_empty() {
            format!(" {ANSI_MAGENTA}[{}]{ANSI_RESET}", span.span)
        } else {
            let mut pairs: Vec<String> = span
                .params
                .iter()
                .map(|(k, v)| format!("{k}={v}"))
                .collect();
            pairs.sort();
            format!(
                " {ANSI_MAGENTA}[{}{{{}}}]{ANSI_RESET}",
                span.span,
                pairs.join(", ")
            )
        }
    }

    fn format_sublevel(&self) -> String {
        match &self.sublevel {
            Some(sub) => format!(" {ANSI_BLUE}[{sub}]{ANSI_RESET}"),
            None => String::new(),
        }
    }

    fn format_message(&self) -> String {
        if self.message.is_empty() {
            return String::new();
        }

        static TAG_RE: OnceLock<Regex> = OnceLock::new();
        let tag_re = TAG_RE.get_or_init(|| Regex::new(r"(\[[A-Za-z0-9_:-]+\])").unwrap());

        let mut lines = self.message.lines();
        let mut out = String::from(" ");

        if let Some(first) = lines.next() {
            let formatted_first = tag_re.replace_all(first, |c: &regex::Captures| {
                format!("{ANSI_MAGENTA}{}{ANSI_RESET}", &c[1])
            });
            out.push_str(&formatted_first);
        }

        for extra in lines {
            let formatted_extra = tag_re.replace_all(extra, |c: &regex::Captures| {
                format!("{ANSI_CYAN}{ANSI_BOLD}{}{ANSI_RESET}", &c[1])
            });
            out.push_str(&format!("\n  {ANSI_DIM}│ {formatted_extra}{ANSI_RESET}"));
        }

        out
    }
}

/// Callback handler invoked for each matched and parsed log entry.
pub type LogHandler = Arc<dyn Fn(Log) + Send + Sync + 'static>;

/// Log record filter.
#[derive(Clone, Debug, Default)]
pub struct Filters {
    pub regexes: Vec<Regex>,
    pub kv_filters: Vec<(String, String)>,
}

impl Filters {
    pub fn new(raw_patterns: &[&str], kv_pairs: &[(&str, &str)]) -> Self {
        let regexes = raw_patterns
            .iter()
            .filter_map(|p| Regex::new(p).ok())
            .collect();

        let kv_filters = kv_pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();

        Self {
            regexes,
            kv_filters,
        }
    }

    /// Evaluates whether a parsed `Log` matches active filters.
    pub fn matches(&self, log: &Log) -> bool {
        if self.regexes.is_empty() && self.kv_filters.is_empty() {
            return true;
        }

        for (k, v) in &self.kv_filters {
            let matched_param = log.parameters.get(k).is_some_and(|val| val == v)
                || log
                    .span
                    .as_ref()
                    .and_then(|s| s.params.get(k))
                    .is_some_and(|val| val == v);
            let matched_msg = {
                let p1 = format!("{k}={v}");
                let p2 = format!("\"{k}\":\"{v}\"");
                let p3 = format!("\"{k}\": {v}");
                log.message.contains(&p1) || log.message.contains(&p2) || log.message.contains(&p3)
            };

            if !matched_param && !matched_msg {
                return false;
            }
        }

        for re in &self.regexes {
            if !re.is_match(&log.message) {
                return false;
            }
        }

        true
    }
}

/// Configuration settings for monitoring an individual log source.
#[derive(Clone)]
pub struct Source {
    /// Display name of the log source.
    pub name: String,
    /// Path to the directory containing log files.
    pub dir_path: PathBuf,
    /// Whether matched logs from this source should be pushed to the print queue.
    pub print: bool,
    /// If true, skips existing file contents on initial open and reads only new lines.
    pub only_new: bool,
}

impl Source {
    pub fn new(
        name: impl Into<String>,
        dir_path: impl Into<PathBuf>,
        print: bool,
        only_new: bool,
    ) -> Self {
        Self {
            name: name.into(),
            dir_path: dir_path.into(),
            print,
            only_new,
        }
    }
}

/// Asynchronous multi-source log tracing supervisor.
pub struct Trace {
    filters: Arc<RwLock<Filters>>,
    print_tx: mpsc::UnboundedSender<String>,
    _handles: Vec<JoinHandle<()>>,
}

impl Trace {
    /// Starts tracing across all provided log sources without a custom handler.
    pub async fn trace(sources: Vec<Source>, poll_interval: Duration, filters: Filters) -> Self {
        Self::trace_with(sources, poll_interval, filters, |_| {}).await
    }

    /// Starts tracing across all provided log sources and invokes `handler` for each matched `Log`.
    pub async fn trace_with<F>(
        sources: Vec<Source>,
        poll_interval: Duration,
        filters: Filters,
        handler: F,
    ) -> Self
    where
        F: Fn(Log) + Send + Sync + 'static,
    {
        let filters = Arc::new(RwLock::new(filters));
        let handler: LogHandler = Arc::new(handler);
        let (print_tx, mut print_rx) = mpsc::unbounded_channel::<String>();
        let mut handles = Vec::with_capacity(sources.len() + 1);

        // Dedicated print queue worker to avoid interleaved terminal writes across sources
        let print_worker = tokio::spawn(async move {
            while let Some(formatted) = print_rx.recv().await {
                println!("{formatted}");
            }
        });
        handles.push(print_worker);

        for config in sources {
            let filters_clone = Arc::clone(&filters);
            let handler_clone = Arc::clone(&handler);
            let print_tx_clone = print_tx.clone();

            let handle = tokio::spawn(async move {
                Self::watch_source(
                    config,
                    poll_interval,
                    filters_clone,
                    print_tx_clone,
                    handler_clone,
                )
                .await;
            });
            handles.push(handle);
        }

        Self {
            filters,
            print_tx,
            _handles: handles,
        }
    }

    /// Replaces active log filtering rules dynamically at runtime.
    pub async fn update_filters(&self, new_filters: Filters) {
        *self.filters.write().await = new_filters;
    }

    /// Enqueues a preformatted string into the serialized print queue.
    pub fn enqueue_print(&self, text: impl Into<String>) {
        let _ = self.print_tx.send(text.into());
    }

    async fn watch_source(
        config: Source,
        interval: Duration,
        filters: Arc<RwLock<Filters>>,
        print_tx: mpsc::UnboundedSender<String>,
        handler: LogHandler,
    ) {
        let header_re = HEADER_REGEX.get();
        let mut current_file: Option<PathBuf> = None;
        let mut reader: Option<BufReader<File>> = None;
        let mut multiline_buf = String::new();

        if !config.dir_path.exists() && config.print {
            let _ = print_tx.send(format!(
                "Directory not found for source {}: {}",
                config.name,
                config.dir_path.display()
            ));
        }

        loop {
            if let Some(latest_file) = Self::find_latest_file(&config.dir_path).await {
                let is_new_file = current_file.as_ref() != Some(&latest_file);

                if is_new_file {
                    Self::flush_buffer(&config, &mut multiline_buf, &filters, &print_tx, &handler)
                        .await;

                    if config.print {
                        let _ = print_tx.send(format!(
                            "Tracing file: {ANSI_BOLD}{}{ANSI_RESET} {ANSI_MAGENTA}{}{ANSI_RESET}",
                            config.name,
                            latest_file.to_string_lossy()
                        ));
                    }

                    if let Ok(file) = File::open(&latest_file).await {
                        let mut new_reader = BufReader::new(file);

                        if config.only_new && current_file.is_none() {
                            let _ = new_reader.seek(SeekFrom::End(0)).await;
                        }

                        reader = Some(new_reader);
                        current_file = Some(latest_file);
                    }
                }
            }

            if let Some(ref mut r) = reader {
                let mut line = String::new();

                while r.read_line(&mut line).await.unwrap_or(0) > 0 {
                    let line_str = line.trim_end_matches(['\r', '\n']);

                    if header_re.is_match(line_str) {
                        Self::flush_buffer(
                            &config,
                            &mut multiline_buf,
                            &filters,
                            &print_tx,
                            &handler,
                        )
                        .await;
                        multiline_buf.push_str(line_str);
                    } else if !multiline_buf.is_empty() {
                        multiline_buf.push('\n');
                        multiline_buf.push_str(line_str);
                    } else {
                        multiline_buf.push_str(line_str);
                    }

                    line.clear();
                }
            }

            sleep(interval).await;
        }
    }

    async fn flush_buffer(
        config: &Source,
        buffer: &mut String,
        filters: &Arc<RwLock<Filters>>,
        print_tx: &mpsc::UnboundedSender<String>,
        handler: &LogHandler,
    ) {
        if buffer.is_empty() {
            return;
        }

        let log = Self::parse_log(&config.name, buffer);

        let matched = {
            let current_filters = filters.read().await;
            current_filters.matches(&log)
        };

        if matched {
            handler(log.clone());

            if config.print {
                let _ = print_tx.send(log.to_string());
            }
        }

        buffer.clear();
    }

    fn parse_meta_blocks(meta: &str) -> (Option<LogSpan>, Option<String>, HashMap<String, String>) {
        let mut span = None;
        let mut sublevel = None;
        let mut params = HashMap::new();

        for caps in BRACKET_BLOCK_RE.get().captures_iter(meta) {
            let inner = caps[1].trim();
            if let Some(scaps) = SPAN_BLOCK_RE.get().captures(inner) {
                let mut span_params = HashMap::new();
                for pair in scaps[2].split(',') {
                    if let Some((k, v)) = pair.split_once('=') {
                        let k = k.trim().trim_matches('"');
                        let v = v.trim().trim_matches('"');
                        if !k.is_empty() {
                            span_params.insert(k.to_string(), v.to_string());
                            params.insert(k.to_string(), v.to_string());
                        }
                    }
                }
                if span.is_none() {
                    span = Some(LogSpan {
                        span: scaps[1].to_string(),
                        params: span_params,
                    });
                }
            } else if sublevel.is_none() {
                sublevel = Some(inner.to_string());
            }
        }

        (span, sublevel, params)
    }

    /// Parses raw multiline log text into a structured `Log` record.
    fn parse_log(source: &str, raw: &str) -> Log {
        let re = HEADER_REGEX.get();
        let mut lines = raw.split('\n');
        let first_line = lines.next().unwrap_or("");
        let rest_lines: Vec<&str> = lines.collect();

        if let Some(caps) = re.captures(first_line) {
            let timestamp = caps.name("time").map(|m| m.as_str().to_string());
            let level = caps
                .name("level")
                .map(|m| normalize_level(m.as_str()).to_string());

            let meta = caps.name("meta").map(|m| m.as_str()).unwrap_or("");
            let (span, sublevel, mut parameters) = Self::parse_meta_blocks(meta);

            let first_msg = caps.name("msg").map(|m| m.as_str()).unwrap_or("");
            let mut message = String::from(first_msg);
            if !rest_lines.is_empty() {
                if !message.is_empty() {
                    message.push('\n');
                }
                message.push_str(&rest_lines.join("\n"));
            }

            for (k, v) in extract_parameters(raw) {
                parameters.entry(k).or_insert(v);
            }

            Log {
                source: source.to_string(),
                timestamp,
                level,
                span,
                sublevel,
                message,
                parameters,
            }
        } else {
            Log {
                source: source.to_string(),
                timestamp: None,
                level: None,
                span: None,
                sublevel: None,
                message: raw.to_string(),
                parameters: extract_parameters(raw),
            }
        }
    }

    async fn find_latest_file(dir: &Path) -> Option<PathBuf> {
        let mut entries = tfs::read_dir(dir).await.ok()?;
        let mut files = Vec::new();

        while let Ok(Some(entry)) = entries.next_entry().await {
            let path = entry.path();
            if path.is_file() {
                files.push(path);
            }
        }

        files.sort();
        files.pop()
    }
}

/// Extracts `key=value` and `"key": value` / `"key":"value"` pairs from log text.
fn extract_parameters(text: &str) -> HashMap<String, String> {
    static PARAM_RE: OnceLock<Regex> = OnceLock::new();
    let re = PARAM_RE.get_or_init(|| {
        Regex::new(
            r#"(?x)
            (?:
                "(?P<jk>[A-Za-z0-9_.-]+)"\s*:\s*"?(?P<jv>[^",\s\}\]]+)"?
                |
                \b(?P<kk>[A-Za-z0-9_.-]+)=(?:"(?P<kvq>[^"]+)"|(?P<kv>[^\s,\]\}]+))
            )
            "#,
        )
        .expect("valid log parameter regex")
    });

    let mut params = HashMap::new();

    for caps in re.captures_iter(text) {
        if let (Some(k), Some(v)) = (caps.name("jk"), caps.name("jv")) {
            params.insert(k.as_str().to_string(), v.as_str().to_string());
        } else if let Some(k) = caps.name("kk") {
            let val = caps
                .name("kvq")
                .or_else(|| caps.name("kv"))
                .map(|m| m.as_str().to_string());
            if let Some(v) = val {
                params.insert(k.as_str().to_string(), v);
            }
        }
    }

    params
}

fn normalize_level(level: &str) -> &str {
    match level {
        "ERR" => "ERROR",
        "WARNING" => "WARN",
        other => other,
    }
}
