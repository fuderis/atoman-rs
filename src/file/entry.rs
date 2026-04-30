use super::{FileKind, Metadata};
use crate::prelude::*;

use tokio::fs::{self, DirEntry};

/// The dir entry structure
#[derive(Debug, Clone)]
pub struct Entry {
    path: PathBuf,
    kind: FileKind,
    inner: Option<Arc<DirEntry>>,
    metadata: Option<Arc<Metadata>>,
}

impl Entry {
    /// Creates a new entry from file path
    pub async fn from_path(path: impl Into<PathBuf>) -> Result<Self> {
        let path = path.into();

        let meta = fs::metadata(&path).await?;
        let kind = if meta.is_dir() {
            FileKind::Dir
        } else if meta.is_file() {
            FileKind::File
        } else {
            FileKind::Symlink
        };

        Ok(Entry {
            path,
            kind,
            inner: None,
            metadata: None,
        })
    }

    /// Creates a new dir entry from tokio DirEntry
    pub async fn from_entry(value: DirEntry) -> Result<Self> {
        let path = value.path();
        let ft = value.file_type().await?;

        let kind = if ft.is_dir() {
            FileKind::Dir
        } else if ft.is_file() {
            FileKind::File
        } else {
            FileKind::Symlink
        };

        Ok(Self {
            path,
            kind,
            inner: Some(arc!(value)),
            metadata: None,
        })
    }

    /// Returns the entry path
    pub fn path(&self) -> &PathBuf {
        &self.path
    }

    /// Reads the entry metadata
    pub async fn metadata(&mut self) -> Result<Arc<Metadata>> {
        if let Some(meta) = &self.metadata {
            Ok(meta.clone())
        } else {
            let std_meta = if let Some(inner) = &self.inner {
                inner.metadata().await?
            } else {
                fs::metadata(&self.path).await?
            };

            let meta = arc!(Metadata::new(&self.path, std_meta)?);
            self.metadata.replace(meta.clone());

            Ok(meta)
        }
    }

    /// Returns the entry file name
    pub fn file_name(&self) -> String {
        self.path
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default()
    }

    /// Returns the entry file type
    pub fn file_type(&self) -> &FileKind {
        &self.kind
    }

    /// Returns true if entry is file
    pub fn is_file(&self) -> bool {
        self.kind.is_file()
    }

    /// Returns true if entry is dir
    pub fn is_dir(&self) -> bool {
        self.kind.is_dir()
    }

    /// Returns true if entry is symlink
    pub fn is_symlink(&self) -> bool {
        self.kind.is_symlink()
    }

    /// Returns the entry file extension
    pub fn extension(&self) -> Option<String> {
        self.path
            .extension()
            .and_then(|s| s.to_str())
            .map(|s| s.to_lowercase())
    }

    /// Returns true if entry is exists
    pub fn exists(&self) -> bool {
        self.path.exists()
    }
}
