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

/// Represents a single structural unit of a log entry.
#[derive(Debug, Clone)]
pub struct LogRecord {
    /// The name or identifier of the log source.
    pub source: String,
    /// The actual log message payload, which can be multiline.
    pub content: String,
}

/// The filtering engine used to evaluate log records against regular expressions
/// and key-value matching pairs.
#[derive(Clone, Debug)]
pub struct FilterEngine {
    /// Compiled regular expressions that every log entry must match.
    pub regexes: Vec<Regex>,
    /// Key-value pairs that must exist within the log message.
    pub kv_filters: Vec<(String, String)>,
}

impl FilterEngine {
    /// Creates a new `FilterEngine` instance from raw regex strings and key-value pairs.
    ///
    /// Patterns that fail to compile as valid regular expressions are silently skipped.
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

    /// Evaluates whether the given log `text` satisfies all configured filters.
    ///
    /// Returns `true` if all key-value patterns and regular expressions match,
    /// or if no filters are defined.
    pub fn matches(&self, text: &str) -> bool {
        if self.regexes.is_empty() && self.kv_filters.is_empty() {
            return true;
        }

        for (k, v) in &self.kv_filters {
            let pattern1 = format!("{}={}", k, v);
            let pattern2 = format!("\"{}\":\"{}\"", k, v);
            let pattern3 = format!("\"{}\": {}", k, v);
            if !text.contains(&pattern1) && !text.contains(&pattern2) && !text.contains(&pattern3) {
                return false;
            }
        }

        for re in &self.regexes {
            if !re.is_match(text) {
                return false;
            }
        }

        true
    }
}

/// Configuration settings for an individual log source.
#[derive(Clone)]
pub struct SourceConfig {
    /// Display name of the log source.
    pub name: String,
    /// Directory path where log files are located.
    pub dir_path: PathBuf,
    /// Regular expression used to identify the starting line of a log entry.
    pub entry_start_pattern: Regex,
    /// Terminal color code assigned for output styling.
    pub color_code: u8,
}

/// Asynchronous engine that monitors multiple log sources and streams filtered records.
pub struct MultiTrace {
    rx: mpsc::UnboundedReceiver<LogRecord>,
    filters: Arc<RwLock<FilterEngine>>,
    _handles: Vec<JoinHandle<()>>,
}

impl MultiTrace {
    /// Starts multi-source log tracing across all configured sources.
    ///
    /// Spawns an asynchronous background task for each `SourceConfig` to monitor file updates
    /// at the specified `poll_interval`.
    pub async fn start(
        sources: Vec<SourceConfig>,
        poll_interval: Duration,
        filters: FilterEngine,
        only_new: bool,
    ) -> Self {
        let (tx, rx) = mpsc::unbounded_channel();
        let filters = Arc::new(RwLock::new(filters));
        let mut _handles = Vec::new();

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

    /// Dynamically updates the active filtering criteria at runtime.
    pub async fn update_filters(&self, new_filters: FilterEngine) {
        *self.filters.write().await = new_filters;
    }

    /// Receives the next available log record from the channel and prints it to stdout.
    pub async fn recv_and_print(&mut self) {
        if let Some(record) = self.rx.recv().await {
            Self::pretty_print(&record);
        }
    }

    /// Watches the specified directory and processes incoming log entries for a single source.
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

        if !config.dir_path.exists() {
            println!(
                "[WARN] Directory not found for source {}: {}",
                config.name,
                config.dir_path.display()
            );
        }

        loop {
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

                        if only_new && current_file.is_none() {
                            let _ = new_reader.seek(SeekFrom::End(0)).await;
                        }

                        reader = Some(new_reader);
                        current_file = Some(latest_file);
                        multiline_buf.clear();
                    }
                }
            }

            if let Some(ref mut r) = reader {
                let mut line = String::new();

                while r.read_line(&mut line).await.unwrap_or(0) > 0 {
                    let trimmed = line.trim_end_matches(['\r', '\n']);

                    if config.entry_start_pattern.is_match(trimmed) {
                        Self::flush_buffer(&config.name, &mut multiline_buf, &filters, &tx).await;
                        multiline_buf.push_str(trimmed);
                    } else if !multiline_buf.is_empty() {
                        multiline_buf.push('\n');
                        multiline_buf.push_str(trimmed);
                    } else {
                        multiline_buf.push_str(trimmed);
                    }

                    line.clear();
                }

                Self::flush_buffer(&config.name, &mut multiline_buf, &filters, &tx).await;
            }

            sleep(interval).await;
        }
    }

    /// Flushes the accumulated multiline log buffer if it passes current filter criteria.
    async fn flush_buffer(
        source_name: &str,
        buffer: &mut String,
        filters: &Arc<RwLock<FilterEngine>>,
        tx: &mpsc::UnboundedSender<LogRecord>,
    ) {
        if buffer.is_empty() {
            return;
        }

        let current_filters = filters.read().await;
        if current_filters.matches(buffer) {
            let _ = tx.send(LogRecord {
                source: source_name.to_string(),
                content: buffer.clone(),
            });
        }
        buffer.clear();
    }

    /// Locates the latest log file in a directory using lexicographical sorting.
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

    /// Prints a formatted and colorized log record to the standard output.
    fn pretty_print(record: &LogRecord) {
        // 1. Compact source name: bold font, no extra padding, no background
        let source_badge = record.source.bold().to_string();

        // Regex for parsing standard log lines
        let log_re = Regex::new(
            r"(?x)
        ^(?P<time>\d{4}-\d{2}-\d{2}[T\s]\d{2}:\d{2}:\d{2}(?:\.\d+)?Z?)?\s*
        (?P<level>TRACE|DEBUG|INFO|WARN|WARNING|ERROR|ERR|FATAL)?\s*
        (?P<target>\[[^\]]+\]|[a-zA-Z0-9_:]+:)?\s*
        (?P<msg>[\s\S]*)
        ",
        )
        .unwrap();

        // Regex for highlighting custom bracketed tags like [Rules], [TAG], etc. in messages
        let tag_re = Regex::new(r"(\[[A-Za-z0-9_:-]+\])").unwrap();

        let mut lines = record.content.lines();

        if let Some(first_line) = lines.next() {
            if let Some(caps) = log_re.captures(first_line) {
                // Timestamp (dimmed)
                let time_str = caps
                    .name("time")
                    .map(|m| format!("{} ", m.as_str().dimmed()))
                    .unwrap_or_default();

                // Log level
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

                // Module / Target: replaced dim purple with bright magenta
                let target_str = caps
                    .name("target")
                    .map(|m| format!("{} ", m.as_str().magenta()))
                    .unwrap_or_default();

                // Message body with highlighted [Bracketed Tags]
                let raw_msg = caps.name("msg").map(|m| m.as_str()).unwrap_or_default();

                // Highlight custom prefixes like [Rules] with bright cyan color
                let msg_str = tag_re.replace_all(raw_msg, |c: &regex::Captures| {
                    c[1].cyan().bold().to_string()
                });

                println!(
                    "{} {}{}{}{}",
                    source_badge, time_str, level_str, target_str, msg_str
                );
            } else {
                // Fallback
                let formatted_line = tag_re.replace_all(first_line, |c: &regex::Captures| {
                    c[1].cyan().bold().to_string()
                });
                println!("{} {}", source_badge, formatted_line);
            }
        }

        // Multiline output (stack traces, JSON) formatted with neat indentation
        for line in lines {
            let formatted_line =
                tag_re.replace_all(line, |c: &regex::Captures| c[1].cyan().bold().to_string());
            println!("  {} {}", "│".dimmed(), formatted_line.dimmed());
        }
    }
}
