use super::*;
use crate::prelude::*;

use bytes::Bytes;
use tokio::fs::{self, File as TokioFile, OpenOptions};
use tokio::io::{
    self, AsyncBufReadExt, AsyncReadExt, AsyncSeekExt, AsyncWriteExt, BufReader,
    SeekFrom as TokioSeekFrom,
};

/// The file open mode
#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash)]
pub enum OpenMode {
    Read,
    Write,
    ReadWrite,
}

/// The file cursor seek options
#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash)]
pub enum SeekFrom {
    Start(u64),
    End(i64),
    Current(i64),
}

impl Into<TokioSeekFrom> for SeekFrom {
    fn into(self) -> TokioSeekFrom {
        match self {
            Self::Start(pos) => TokioSeekFrom::Start(pos),
            Self::End(pos) => TokioSeekFrom::End(pos),
            Self::Current(pos) => TokioSeekFrom::Current(pos),
        }
    }
}

impl From<TokioSeekFrom> for SeekFrom {
    fn from(value: TokioSeekFrom) -> Self {
        match value {
            TokioSeekFrom::Start(pos) => Self::Start(pos),
            TokioSeekFrom::End(pos) => Self::End(pos),
            TokioSeekFrom::Current(pos) => Self::Current(pos),
        }
    }
}

/// The file reader/writer
#[derive(Debug)]
pub struct File {
    pub path: PathBuf,
    pub metadata: Option<Metadata>,
    file: BufReader<TokioFile>,
}

impl File {
    /// Open file with open options
    pub async fn open(path: impl AsRef<Path>, mode: OpenMode) -> Result<Self> {
        let path = path.as_ref().to_path_buf();
        let mut options = OpenOptions::new();

        // set the file open mode
        match mode {
            OpenMode::Read => {
                options.read(true);
            }
            OpenMode::Write => {
                options.create(true).write(true);
            }
            OpenMode::ReadWrite => {
                options.create(true).read(true).write(true);
            }
        }

        // create parent dir if not exists (for write mode)
        if mode != OpenMode::Read {
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).await?;
            }
        }

        // open raw file
        let raw_file = options.open(&path).await?;
        let mut file = Self {
            // creating a BufReader with the default buffer size (8KB)
            file: BufReader::new(raw_file),
            metadata: None,
            path,
        };

        // update metadata
        file.refresh().await?;

        Ok(file)
    }

    /// Open file for read
    pub async fn open_read(path: impl AsRef<Path>) -> Result<Self> {
        Self::open(path, OpenMode::Read).await
    }

    /// Open file for write
    pub async fn open_write(path: impl AsRef<Path>) -> Result<Self> {
        Self::open(path, OpenMode::Write).await
    }

    /// Open file for read & write
    pub async fn open_read_write(path: impl AsRef<Path>) -> Result<Self> {
        Self::open(path, OpenMode::ReadWrite).await
    }

    /// Updates the file metadata
    pub async fn refresh(&mut self) -> Result<()> {
        self.metadata = Some(Metadata::new(
            &self.path,
            self.file.get_ref().metadata().await?,
        )?);
        Ok(())
    }

    /// Sets the cursor position
    pub async fn seek(&mut self, seek: impl Into<TokioSeekFrom>) -> Result<u64> {
        // WARNING: When using BufReader, a regular seek will flush the internal buffer
        // (this is normal, but it must be remembered)
        let cursor = self.file.seek(seek.into()).await?;
        Ok(cursor)
    }

    /// Sets the cursor position from the file start
    pub async fn seek_start(&mut self, pos: u64) -> Result<u64> {
        self.seek(SeekFrom::Start(pos)).await
    }

    /// Sets the cursor position from the file end
    pub async fn seek_end(&mut self, pos: i64) -> Result<u64> {
        self.seek(SeekFrom::End(pos)).await
    }

    /// Sets the cursor position from the current position
    pub async fn seek_current(&mut self, pos: i64) -> Result<u64> {
        self.seek(SeekFrom::Current(pos)).await
    }

    /// Returns the current position
    pub async fn position(&mut self) -> Result<u64> {
        Ok(self.file.stream_position().await?)
    }

    /// Reads some bytes of contents
    pub async fn read(&mut self) -> Result<Option<Bytes>> {
        let mut buffer = vec![0u8; 8096];
        match self.file.read(&mut buffer).await {
            Ok(0) => Ok(None),
            Ok(n) => {
                buffer.truncate(n);
                Ok(Some(Bytes::from(buffer)))
            }
            Err(e) => Err(e.into()),
        }
    }

    /// Reads N-bytes of contents (returns Error if EOF before filling the buffer)
    pub async fn read_exact(&mut self, len: usize) -> Result<Option<Bytes>> {
        if len == 0 {
            return Ok(Some(Bytes::new()));
        }

        let mut buffer = vec![0u8; len];
        match self.file.read_exact(&mut buffer).await {
            Ok(_) => Ok(Some(Bytes::from(buffer))),
            Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    /// Reads data until a specific byte is found (Optimized with BufReader)
    pub async fn read_until(&mut self, stop_byte: u8) -> Result<Option<Bytes>> {
        let mut buffer = Vec::new();
        // we use the native BufReader method. He reads a bunch at once into memory
        let n = self.file.read_until(stop_byte, &mut buffer).await?;

        if n == 0 {
            return Ok(None);
        }

        // if we find a stop byte, we delete it from the result
        if buffer.last() == Some(&stop_byte) {
            buffer.pop();
        }

        Ok(Some(Bytes::from(buffer)))
    }

    /// Reads until a sequence of bytes is found (Non-optimized, pattern search)
    /// * capt_eof=true: returns the remaining data before EOF (even if this doesn't fit the stop-bytes pattern)
    pub async fn read_until_pattern(
        &mut self,
        stop_bytes: &[u8],
        capt_eof: bool,
    ) -> Result<Option<Bytes>> {
        let mut buffer = Vec::<u8>::with_capacity(8096);
        let mut chunk = [0u8; 1024];
        let stop_len = stop_bytes.len();

        loop {
            // BufReader will still speed up this process, as read() will take data from memory
            let n = match self.file.read(&mut chunk).await {
                Ok(0) => {
                    if capt_eof && !buffer.is_empty() {
                        return Ok(Some(Bytes::from(buffer)));
                    }
                    if !buffer.is_empty() {
                        self.seek_current(-(buffer.len() as i64)).await?;
                    }
                    return Ok(None);
                }
                Ok(n) => n,
                Err(e) => return Err(e.into()),
            };

            buffer.extend_from_slice(&chunk[..n]);

            let search_start = buffer.len().saturating_sub(n + stop_len - 1);
            if let Some(rel_pos) = buffer[search_start..]
                .windows(stop_len)
                .position(|w| w == stop_bytes)
            {
                let abs_pos = search_start + rel_pos;
                let match_end = abs_pos + stop_len;
                let over_read = buffer.len() - match_end;

                // returning the extra read bytes back to the stream
                if over_read > 0 {
                    self.seek_current(-(over_read as i64)).await?;
                }

                buffer.truncate(abs_pos);
                return Ok(Some(Bytes::from(buffer)));
            }
        }
    }

    /// Reads the next line (Optimized)
    pub async fn read_line(&mut self) -> Result<Option<Bytes>> {
        self.read_until(b'\n').await
    }

    /// Reads the all next lines
    pub async fn read_lines(&mut self) -> Result<Vec<Bytes>> {
        let mut lines = Vec::new();
        while let Some(line) = self.read_line().await? {
            lines.push(line);
        }
        Ok(lines)
    }

    /// Reads the remaining file contents
    pub async fn read_to_end(&mut self) -> Result<Vec<u8>> {
        let mut data = Vec::new();
        self.file.read_to_end(&mut data).await?;
        Ok(data)
    }

    /// Reads the remaining file contents to string
    pub async fn read_to_string(&mut self) -> Result<String> {
        let bytes = self.read_to_end().await?;
        let s = String::from_utf8_lossy(&bytes).to_string();
        Ok(s)
    }

    /// Writes a bytes after the cursor (returns written bytes count)
    pub async fn write(&mut self, data: &[u8]) -> Result<usize> {
        let written = self.file.write(data).await?;
        Ok(written)
    }

    /// Guaranteed writes the all bytes after the cursor
    pub async fn write_all(&mut self, data: &[u8]) -> Result<()> {
        self.file.write_all(data).await?;
        Ok(())
    }

    /// Re-writes the full file contents from start
    pub async fn rewrite_all(&mut self, data: &[u8]) -> Result<()> {
        self.seek(SeekFrom::Start(0)).await?;
        self.file.get_mut().set_len(0).await?;
        self.write_all(data).await?;
        self.file.flush().await?;
        Ok(())
    }

    /// Cuts the file contents by new size
    pub async fn cut(&mut self, new_size: u64) -> Result<()> {
        self.file.get_mut().set_len(new_size).await?;
        Ok(())
    }

    /// Force write changes to file
    pub async fn flush(&mut self) -> Result<()> {
        self.file.flush().await?;
        Ok(())
    }

    /// Force write changes to file with metadata
    pub async fn sync_all(&mut self) -> Result<()> {
        self.file.get_mut().sync_all().await?;
        Ok(())
    }

    /// Removes the file
    pub async fn remove(path: impl AsRef<Path>) -> Result<()> {
        Ok(fs::remove_file(path).await?)
    }

    /// Removes the current file
    pub async fn remove_self(self) -> Result<()> {
        let path = self.path;
        drop(self.file);
        Self::remove(path).await
    }
}
