use colored::*;
use regex::Regex;
use std::{
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};
use tokio::{
    fs::{self as tfs, File},
    io::{AsyncBufReadExt, AsyncSeekExt, BufReader, SeekFrom},
    sync::{RwLock, mpsc},
    task::JoinHandle,
    time::sleep,
};

/// Default regular expression pattern used to detect the start of a new log entry.
///
/// Ensures the line begins strictly without leading whitespace (`^`), followed by an ISO timestamp
/// and a valid log severity level (e.g., `2026-08-26T23:36:44Z INFO`).
pub const ENTRY_START_PATTERN: &str = r"^\d{4}-\d{2}-\d{2}[T\s]\d{2}:\d{2}:\d{2}(?:\.\d+)?Z?\s+(?:TRACE|DEBUG|INFO|WARN|WARNING|ERROR|ERR|FATAL)\b";

/// Represents a single structural log record collected from a source.
#[derive(Debug, Clone)]
pub struct LogRecord {
    /// The name or identifier of the log source.
    pub source: String,
    /// The raw payload of the log entry, which may contain multiple lines.
    pub content: String,
}

/// Filtering engine that evaluates log records against regular expressions and key-value criteria.
#[derive(Clone, Debug)]
pub struct FilterEngine {
    /// Compiled regular expressions that every log entry must match.
    pub regexes: Vec<Regex>,
    /// Key-value pairs required to exist within the log message text.
    pub kv_filters: Vec<(String, String)>,
}

impl FilterEngine {
    /// Creates a new `FilterEngine` from raw regex strings and key-value tuple pairs.
    ///
    /// Patterns that fail to compile as valid regular expressions are silently ignored.
    pub fn new(raw_patterns: &[&str], kv_pairs: &[(&str, &str)]) -> Self {
        // filter out invalid regex patterns
        let regexes = raw_patterns
            .iter()
            .filter_map(|p| Regex::new(p).ok())
            .collect();

        // convert key-value reference pairs into owned string tuples
        let kv_filters = kv_pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();

        Self {
            regexes,
            kv_filters,
        }
    }

    /// Evaluates whether the provided `text` satisfies all active filtering criteria.
    ///
    /// Returns `true` if all regular expressions and key-value conditions match,
    /// or if no filters are configured.
    pub fn matches(&self, text: &str) -> bool {
        // short-circuit if no filters are specified
        if self.regexes.is_empty() && self.kv_filters.is_empty() {
            return true;
        }

        // evaluate key-value filter patterns
        for (k, v) in &self.kv_filters {
            let pattern1 = format!("{}={}", k, v);
            let pattern2 = format!("\"{}\":\"{}\"", k, v);
            let pattern3 = format!("\"{}\": {}", k, v);
            if !text.contains(&pattern1) && !text.contains(&pattern2) && !text.contains(&pattern3) {
                return false;
            }
        }

        // evaluate regular expression patterns
        for re in &self.regexes {
            if !re.is_match(text) {
                return false;
            }
        }

        true
    }
}

/// Configuration settings for monitoring an individual log source.
#[derive(Clone)]
pub struct SourceConfig {
    /// Display name of the log source.
    pub name: String,
    /// Path to the directory containing log files.
    pub dir_path: PathBuf,
    /// Regular expression used to identify the beginning of a discrete log entry.
    pub entry_start_pattern: Regex,
    /// ANSI terminal color code used for output styling.
    pub color_code: u8,
}

impl SourceConfig {
    /// Creates a new `SourceConfig` instance using the default log entry start pattern.
    pub fn new(name: impl Into<String>, dir_path: impl Into<PathBuf>, color_code: u8) -> Self {
        Self {
            name: name.into(),
            dir_path: dir_path.into(),
            entry_start_pattern: Regex::new(ENTRY_START_PATTERN).unwrap(),
            color_code,
        }
    }
}

/// Asynchronous multi-source log tracing supervisor.
pub struct MultiTrace {
    rx: mpsc::UnboundedReceiver<LogRecord>,
    filters: Arc<RwLock<FilterEngine>>,
    _handles: Vec<JoinHandle<()>>,
}

impl MultiTrace {
    /// Starts tracing across all provided log sources asynchronously.
    ///
    /// Spawns a background task for each source configuration to continuously monitor target files
    /// at the given `poll_interval`.
    pub async fn start(
        sources: Vec<SourceConfig>,
        poll_interval: Duration,
        filters: FilterEngine,
        only_new: bool,
    ) -> Self {
        // establish channel for receiving aggregated log records
        let (tx, rx) = mpsc::unbounded_channel();
        let filters = Arc::new(RwLock::new(filters));
        let mut _handles = Vec::new();

        // spawn monitoring task per source configuration
        for config in sources {
            let tx_clone = tx.clone();
            let filters_clone = filters.clone();

            let handle = tokio::spawn(async move {
                Self::watch_source(config, poll_interval, filters_clone, tx_clone, only_new).await;
            });
            _handles.push(handle);
        }

        Self {
            rx,
            filters,
            _handles,
        }
    }

    /// Replaces active log filtering rules dynamically at runtime.
    pub async fn update_filters(&self, new_filters: FilterEngine) {
        // update shared filter engine under a write lock
        *self.filters.write().await = new_filters;
    }

    /// Awaits the next incoming log record from the stream and prints it to stdout.
    pub async fn recv_and_print(&mut self) {
        // receive next record from channel and send to pretty printer
        if let Some(record) = self.rx.recv().await {
            Self::pretty_print(&record);
        }
    }

    /// Internal asynchronous loop watching a directory for updates on a single source.
    async fn watch_source(
        config: SourceConfig,
        interval: Duration,
        filters: Arc<RwLock<FilterEngine>>,
        tx: mpsc::UnboundedSender<LogRecord>,
        only_new: bool,
    ) {
        let mut current_file: Option<PathBuf> = None;
        let mut reader: Option<BufReader<File>> = None;
        let mut multiline_buf = String::new();

        // check directory existence upfront
        if !config.dir_path.exists() {
            println!(
                "[WARN] Directory not found for source {}: {}",
                config.name,
                config.dir_path.display()
            );
        }

        loop {
            // resolve the latest file in the target directory
            if let Some(latest_file) = Self::find_latest_file(&config.dir_path).await {
                let is_new_file = current_file.as_ref() != Some(&latest_file);

                if is_new_file {
                    println!(
                        "[INFO] Tracing file ({}) {}",
                        config.name,
                        latest_file.display()
                    );

                    if let Ok(file) = File::open(&latest_file).await {
                        let mut new_reader = BufReader::new(file);

                        // seek to end if requested on initial open
                        if only_new && current_file.is_none() {
                            let _ = new_reader.seek(SeekFrom::End(0)).await;
                        }

                        reader = Some(new_reader);
                        current_file = Some(latest_file);
                        multiline_buf.clear();
                    }
                }
            }

            // process newly available lines from reader
            if let Some(ref mut r) = reader {
                let mut line = String::new();

                while r.read_line(&mut line).await.unwrap_or(0) > 0 {
                    // trim line endings while preserving leading whitespace
                    let line_str = line.trim_end_matches(['\r', '\n']);

                    // test for log entry boundary using raw line structure
                    if config.entry_start_pattern.is_match(line_str) {
                        Self::flush_buffer(&config.name, &mut multiline_buf, &filters, &tx).await;
                        multiline_buf.push_str(line_str);
                    } else if !multiline_buf.is_empty() {
                        multiline_buf.push('\n');
                        multiline_buf.push_str(line_str);
                    } else {
                        // handle orphan lines at file start
                        multiline_buf.push_str(line_str);
                    }

                    line.clear();
                }

                // flush remaining entries in buffer
                Self::flush_buffer(&config.name, &mut multiline_buf, &filters, &tx).await;
            }

            sleep(interval).await;
        }
    }

    /// Flushes the active multiline buffer to the receiver channel if it matches current filters.
    async fn flush_buffer(
        source_name: &str,
        buffer: &mut String,
        filters: &Arc<RwLock<FilterEngine>>,
        tx: &mpsc::UnboundedSender<LogRecord>,
    ) {
        if buffer.is_empty() {
            return;
        }

        // evaluate buffer against dynamic filter configuration
        let current_filters = filters.read().await;
        if current_filters.matches(buffer) {
            let _ = tx.send(LogRecord {
                source: source_name.to_string(),
                content: buffer.clone(),
            });
        }
        buffer.clear();
    }

    /// Locates the lexicographically latest file in the specified directory.
    async fn find_latest_file(dir: &Path) -> Option<PathBuf> {
        let mut entries = tfs::read_dir(dir).await.ok()?;
        let mut files = Vec::new();

        // collect all file entries in directory
        while let Ok(Some(entry)) = entries.next_entry().await {
            let path = entry.path();
            if path.is_file() {
                files.push(path);
            }
        }

        // sort lexicographically and extract latest path
        files.sort();
        files.pop()
    }

    /// Formats and outputs a structured log record to standard output with color accents.
    fn pretty_print(record: &LogRecord) {
        let source_badge = record.source.bold().to_string();

        // pattern for parsing structured log header fields
        let log_re = Regex::new(
            r"(?x)
            ^(?P<time>\d{4}-\d{2}-\d{2}[T\s]\d{2}:\d{2}:\d{2}(?:\.\d+)?Z?)\s+
            (?P<level>TRACE|DEBUG|INFO|WARN|WARNING|ERROR|ERR|FATAL)\s*
            (?P<target>\[[^\]]+\]|[a-zA-Z0-9_:]+:)?\s*
            (?P<msg>[\s\S]*)
            ",
        )
        .unwrap();

        let tag_re = Regex::new(r"(\[[A-Za-z0-9_:-]+\])").unwrap();
        let mut lines = record.content.lines();

        // format and render primary header line
        if let Some(first_line) = lines.next() {
            if let Some(caps) = log_re.captures(first_line) {
                // format timestamp metadata
                let time_str = caps
                    .name("time")
                    .map(|m| format!("{} ", m.as_str().dimmed()))
                    .unwrap_or_default();

                // format severity level with color coding
                let level_str = caps
                    .name("level")
                    .map(|m| {
                        let l = m.as_str();
                        (match l {
                            "ERROR" | "ERR" | "FATAL" => l.red().bold().to_string(),
                            "WARN" | "WARNING" => l.yellow().bold().to_string(),
                            "INFO" => l.green().bold().to_string(),
                            "DEBUG" => l.blue().bold().to_string(),
                            "TRACE" => l.magenta().dimmed().to_string(),
                            _ => l.normal().to_string(),
                        }) + " "
                    })
                    .unwrap_or_default();

                // format target module prefix
                let target_str = caps
                    .name("target")
                    .map(|m| format!("{} ", m.as_str().magenta()))
                    .unwrap_or_default();

                // colorize custom bracketed tags in message payload
                let raw_msg = caps.name("msg").map(|m| m.as_str()).unwrap_or_default();
                let msg_str = tag_re.replace_all(raw_msg, |c: &regex::Captures| {
                    c[1].cyan().bold().to_string()
                });

                println!(
                    "{} {}{}{}{}",
                    source_badge, time_str, level_str, target_str, msg_str
                );
            } else {
                // fallback printing for unstructured first line
                let formatted_line = tag_re.replace_all(first_line, |c: &regex::Captures| {
                    c[1].cyan().bold().to_string()
                });
                println!("{} {}", source_badge, formatted_line);
            }
        }

        // render nested multiline content (stack traces, JSON) with indentation guide
        for line in lines {
            let formatted_line =
                tag_re.replace_all(line, |c: &regex::Captures| c[1].cyan().bold().to_string());
            println!("  {} {}", "│".dimmed(), formatted_line.dimmed());
        }
    }
}
