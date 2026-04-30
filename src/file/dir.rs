use super::*;
use crate::prelude::*;

use chrono::{DateTime, Utc};
use tokio::fs;

#[cfg(feature = "search")]
use regex::Regex;

/// The directory manager
pub struct Dir {
    pub path: PathBuf,
    pub metadata: Option<Metadata>,
    pub entries: Vec<Entry>,
}

impl Dir {
    /// Opens the dir and reads his the entries
    pub async fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref().to_path_buf();

        // create dir struct:
        let mut instance = Self {
            path,
            metadata: None,
            entries: vec![],
        };

        // update metadata & read entries:
        instance.refresh().await?;

        Ok(instance)
    }

    /// Update the dir metadata & re-read entries
    pub async fn refresh(&mut self) -> Result<()> {
        self.metadata = Some(Metadata::new(&self.path, fs::metadata(&self.path).await?)?);
        self.entries = Self::read_all(&self.path).await?;
        Ok(())
    }

    /// Returns true if the dir metadata updated
    pub async fn changed(&self) -> bool {
        if let Some(meta) = &self.metadata
            && let Ok(m) = fs::metadata(&self.path).await
            && let Ok(modified_system) = m.modified()
        {
            // converting SystemTime from FS to DateTime<Utc> for comparison:
            let current_modified: DateTime<Utc> = modified_system.into();
            return current_modified != meta.modified;
        }

        true
    }

    /// Creates dir with all the subdirs
    pub async fn create_all(path: impl AsRef<Path>) -> Result<()> {
        Ok(fs::create_dir_all(path).await?)
    }

    /// Creates dir with all the subdirs (without filename)
    pub async fn create_all_parents(path: impl AsRef<Path>) -> Result<()> {
        if let Some(parent) = path.as_ref().parent() {
            fs::create_dir_all(parent).await?;
        }
        Ok(())
    }

    /// Returns the entries reader
    pub async fn read(path: impl AsRef<Path>) -> Result<Entries> {
        let inner = fs::read_dir(path).await?;
        Ok(Entries::new(inner))
    }

    /// Reads & returns all the entries
    pub async fn read_all(path: impl AsRef<Path>) -> Result<Vec<Entry>> {
        let mut entries = Self::read(path).await?;
        let mut results = vec![];

        while let Some(entry) = entries.next_entry().await? {
            results.push(entry);
        }

        Ok(results)
    }

    /// Reads & returns only all the dirs
    pub async fn read_all_dirs(path: impl AsRef<Path>) -> Result<Vec<Entry>> {
        let mut entries = Self::read(path).await?;
        let mut results = vec![];

        while let Some(entry) = entries.next_entry().await? {
            if entry.file_type().is_dir() {
                results.push(entry);
            }
        }

        Ok(results)
    }

    /// Reads & returns only all the files
    pub async fn read_all_files(path: impl AsRef<Path>) -> Result<Vec<Entry>> {
        let mut entries = Self::read(path).await?;
        let mut results = vec![];

        while let Some(entry) = entries.next_entry().await? {
            if entry.file_type().is_file() {
                results.push(entry);
            }
        }

        Ok(results)
    }

    /// Reads all the entries with the separated handlers for files and dirs
    pub async fn read_map<F, D>(path: impl AsRef<Path>, mut on_file: F, mut on_dir: D) -> Result<()>
    where
        F: FnMut(Entry),
        D: FnMut(Entry),
    {
        let mut entries = Self::read(path).await?;
        while let Some(entry) = entries.next_entry().await? {
            let ft = entry.file_type();

            if ft.is_symlink() {
                continue;
            }

            if ft.is_dir() {
                on_dir(entry);
            } else {
                on_file(entry);
            }
        }

        Ok(())
    }

    /// Removes the dir with all the entries
    pub async fn remove(path: impl AsRef<Path>) -> Result<()> {
        Ok(fs::remove_dir_all(path).await?)
    }
}

#[cfg(feature = "search")]
impl Dir {
    /// Search on entries by regular expression
    pub fn search_regex(&self, re: &Regex, files_only: bool) -> Vec<PathBuf> {
        self.entries
            .iter()
            .filter(|e| e.kind != FileKind::Symlink)
            .filter(|e| !files_only || e.kind == FileKind::File)
            .filter(|e| re.is_match(&e.file_name()))
            .map(|e| e.path.clone())
            .collect()
    }

    /// Search on entries by modified Levenshtein distance
    pub fn search(&self, pattern: &str, coef: f32, files_only: bool) -> Vec<PathBuf> {
        let candidates: Vec<_> = self
            .entries
            .iter()
            .filter(|e| e.kind != FileKind::Symlink)
            .filter(|e| !files_only || e.kind == FileKind::File)
            .collect();

        fuzzy_cmp::search_filter(&candidates, pattern, coef, true, |e| {
            e.path.file_name().and_then(|s| s.to_str()).unwrap_or("")
        })
        .into_iter()
        .map(|(_, e)| e.path.clone())
        .collect()
    }

    /// Deep search on entries and sub-entries by modified Levenshtein distance
    pub async fn deep_search(
        &self,
        pattern: &str,
        coef: f32,
        files_only: bool,
    ) -> Result<Vec<PathBuf>> {
        // searching at current level (using the cache):
        let mut results = self.search(pattern, coef, files_only);

        // if current level is empty, go deeper:
        let subdirs: Vec<&Entry> = self
            .entries
            .iter()
            .filter(|e| e.kind == FileKind::Dir)
            .collect();

        for dir_entry in subdirs {
            // open subdirectory (this will create a new Dir object and read its entries):
            let subdir = Dir::open(&dir_entry.path).await?;

            // recursively call deep_search for the subfolder:
            if let Ok(subresults) = Box::pin(subdir.deep_search(pattern, coef, files_only)).await {
                results.extend(subresults);
            }
        }

        Ok(results)
    }
}
