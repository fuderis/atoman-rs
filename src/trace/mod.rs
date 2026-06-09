use crate::{flag::Flag, prelude::*, state::State};
use std::{
    collections::VecDeque,
    path::{Path, PathBuf},
    sync::Arc,
};
use tokio::{
    fs::{self as tfs, File},
    io::{self as tio, AsyncBufReadExt, AsyncSeekExt, BufReader},
    sync::Mutex,
    task::JoinHandle,
    time::{Duration, sleep},
};

/// High-performance async log tracer with dynamic filtering
#[derive(Debug, Clone)]
pub struct Trace {
    path: PathBuf,
    #[allow(dead_code)]
    reader: Arc<Mutex<BufReader<File>>>,
    stack: Arc<State<VecDeque<String>>>,
    available: Arc<Flag>,
    filters: Arc<State<Vec<String>>>,
    _reader_handle: Arc<JoinHandle<()>>,
}

impl Trace {
    /// Returns file path
    pub fn get_path(&self) -> &PathBuf {
        &self.path
    }

    /// Updates filters on the fly (e.g. when admin changes target user)
    pub async fn update_filters(&self, new_filters: Vec<String>) {
        *self.filters.lock().await = new_filters;
    }

    /// Opens file and starts background metadata polling task
    pub async fn open<P: AsRef<Path>>(
        file_path: P,
        timeout: Duration,
        filters: Vec<String>,
        only_new: bool,
    ) -> Result<Self> {
        let path = file_path.as_ref().to_path_buf();
        let file = File::open(&file_path).await.map_err(Error::OpenFile)?;
        let mut reader = BufReader::new(file);

        if only_new {
            let _ = reader.seek(tio::SeekFrom::End(0)).await;
        }

        let reader = Arc::new(Mutex::new(reader));
        let stack = Arc::new(State::from(VecDeque::with_capacity(32)));
        let available = Arc::new(Flag::from(false));
        let filters = Arc::new(State::from(filters));

        if !only_new {
            let mut r = reader.lock().await;
            let current_filters = filters.dirty_get();
            if let Ok(initial_lines) = Self::read_available_lines(&mut r, &current_filters).await {
                if !initial_lines.is_empty() {
                    stack.lock().await.extend(initial_lines);
                    available.set(true);
                }
            }
        }

        let path_clone = path.clone();
        let reader_clone = reader.clone();
        let stack_clone = stack.clone();
        let available_clone = available.clone();
        let filters_clone = filters.clone();

        let reader_handle = tokio::spawn(async move {
            let mut last_mod = tfs::metadata(&path_clone)
                .await
                .ok()
                .and_then(|m| m.modified().ok())
                .unwrap_or(std::time::UNIX_EPOCH);

            let mut last_size = tfs::metadata(&path_clone)
                .await
                .map(|m| m.len())
                .unwrap_or(0);

            loop {
                if let Ok(meta) = tfs::metadata(&path_clone).await {
                    let mod_time = meta.modified().unwrap_or(std::time::UNIX_EPOCH);
                    let current_size = meta.len();

                    if current_size < last_size {
                        if let Ok(new_file) = File::open(&path_clone).await {
                            let mut r = reader_clone.lock().await;
                            *r = BufReader::new(new_file);
                            last_size = 0;
                        }
                    }

                    if mod_time > last_mod || current_size > last_size {
                        let mut r = reader_clone.lock().await;
                        let current_filters = filters_clone.lock().await.clone();

                        if let Ok(new_lines) =
                            Self::read_available_lines(&mut r, &current_filters).await
                            && !new_lines.is_empty()
                        {
                            stack_clone.lock().await.extend(new_lines);
                            if available_clone.is_false() {
                                available_clone.set(true);
                            }
                        }
                        last_mod = mod_time;
                        last_size = current_size;
                    }
                }

                sleep(timeout).await;
            }
        });

        Ok(Self {
            path,
            reader,
            stack,
            available,
            filters,
            _reader_handle: Arc::new(reader_handle),
        })
    }

    /// Fast non-blocking check for available lines in the stack
    pub async fn try_read(&self) -> Option<Vec<String>> {
        let mut lines = vec![];
        let mut s = self.stack.lock().await;

        while let Some(line) = s.pop_front() {
            lines.push(line);
        }

        if lines.is_empty() {
            None
        } else {
            if self.available.is_true() {
                self.available.set(false);
            }
            Some(lines)
        }
    }

    /// Async stream reader (waits until filtered lines appear)
    pub async fn read(&self) -> Option<Vec<String>> {
        loop {
            while self.available.is_false() {
                self.available.wait(true).await;
            }

            let mut lines = vec![];
            let mut s = self.stack.lock().await;
            while let Some(line) = s.pop_front() {
                lines.push(line);
            }

            if lines.is_empty() {
                self.available.set(false);
            } else {
                return Some(lines);
            }
        }
    }

    /// Reads all lines from the current position to EOF with intersection filter
    async fn read_available_lines(
        reader: &mut BufReader<File>,
        filters: &[String],
    ) -> Result<VecDeque<String>> {
        let mut new_lines = VecDeque::new();
        let mut line = String::new();

        while reader.read_line(&mut line).await.map_err(Error::ReadFile)? > 0 {
            if line.ends_with('\n') {
                line.pop();
            }
            if line.ends_with('\r') {
                line.pop();
            }

            let is_match = filters.is_empty() || filters.iter().all(|f| line.contains(f));

            if is_match && !line.is_empty() {
                new_lines.push_back(line.clone());
            }
            line.clear();
        }

        Ok(new_lines)
    }
}
