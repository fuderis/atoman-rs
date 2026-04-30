use crate::prelude::*;
use tokio::fs::ReadDir;

/// A dir entries iterator
pub struct Entries {
    inner: ReadDir,
}

impl Entries {
    /// Creates a new entries iterator
    pub fn new(inner: ReadDir) -> Self {
        Self { inner }
    }

    /// Reads a next dir entry
    pub async fn next(&mut self) -> Result<Option<Entry>> {
        if let Some(entry) = self.inner.next_entry().await? {
            return Ok(Some(Entry::from_entry(entry).await?));
        }
        Ok(None)
    }

    /// Reads a next dir entry
    pub async fn next_entry(&mut self) -> Result<Option<Entry>> {
        self.next().await
    }

    /// Reads a next dir entry with file_type=file (skip subdirectories & symlinks)
    pub async fn next_file(&mut self) -> Result<Option<Entry>> {
        while let Some(entry) = self.next_entry().await? {
            if entry.file_type().is_file() {
                return Ok(Some(entry));
            }
        }
        Ok(None)
    }

    /// Reads a next dir entry with file_type=dir (skip files & symlinks)
    pub async fn next_dir(&mut self) -> Result<Option<Entry>> {
        while let Some(entry) = self.next_entry().await? {
            if entry.file_type().is_dir() {
                return Ok(Some(entry));
            }
        }
        Ok(None)
    }

    /// Reads a next dir entry with file_type=symlink (skip files & subdirectories)
    pub async fn next_symlink(&mut self) -> Result<Option<Entry>> {
        while let Some(entry) = self.next_entry().await? {
            if entry.file_type().is_symlink() {
                return Ok(Some(entry));
            }
        }
        Ok(None)
    }
}
