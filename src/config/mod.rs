use crate::{State, prelude::*};

use chrono::{DateTime, Utc};
use serde::{Serialize, de::DeserializeOwned};
use std::{
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::fs;

/// The config modify metadata
#[derive(Default, Debug, Clone)]
struct Modify {
    modified: Option<DateTime<Utc>>,
    checked: Option<Instant>,
}

impl Modify {
    /// Creates a new Modify instance
    pub fn now() -> Self {
        Self {
            modified: Some(Utc::now()),
            checked: Some(Instant::now()),
        }
    }
}

/// The atomic config wrapper
#[derive(Default, Clone)]
pub struct Config<
    T: Default + Debugging + Clone + Serialize + DeserializeOwned + Send + Sync + 'static,
> {
    path: PathBuf,
    data: T,
    modify: Arc<State<Modify>>,
}

impl<T> Config<T>
where
    T: Clone + Default + Debugging + Serialize + DeserializeOwned + Send + Sync + 'static,
{
    /// Reads the config file or creates the default
    pub async fn new<P: Into<PathBuf>>(file_path: P) -> Result<Self> {
        let file_path = file_path.into();

        // reading the config file
        let this = if file_path.exists() {
            Self::read(&file_path).await?
        }
        // writing the default config file
        else {
            let mut this = Config::<T>::default();
            this.write(file_path).await?;
            this
        };

        Ok(this)
    }

    /// Returns the config file path
    pub fn path(&self) -> &PathBuf {
        &self.path
    }

    /// Parses the config from a raw text
    pub fn parse<P: Into<PathBuf>>(file_path: P, contents: &str) -> Result<Self> {
        let path = file_path.into();

        let data: T = match path
            .extension()
            .and_then(|s| s.to_str())
            .unwrap_or("TOML")
            .to_uppercase()
            .as_str()
        {
            #[cfg(any(feature = "toml-config"))]
            "TOML" => toml::from_str(contents).map_err(|e| Error::ParseConfig(Box::new(e)))?,

            #[cfg(any(feature = "json-config"))]
            "JSON" => {
                serde_json::from_str(contents).map_err(|e| Error::ParseConfig(Box::new(e)))?
            }

            ext => return Err(Error::ConfigExt(ext.to_owned()).into()),
        };

        Ok(Self {
            path,
            data,
            modify: arc!(Modify::now().into()),
        })
    }

    /// Reads the config file
    pub async fn read<P: Into<PathBuf>>(file_path: P) -> Result<Self> {
        let file_path = file_path.into();

        // reading the config file
        let contents = fs::read_to_string(&file_path).await?;
        Self::parse(file_path, &contents)
    }

    /// Saves the config to custom file path safely using a temp file
    pub async fn write<P: Into<PathBuf>>(&mut self, file_path: P) -> Result<()> {
        self.path = file_path.into();

        // 1. Definition of extension and serialization
        let contents = match self
            .path
            .extension()
            .map(|s| s.to_str().unwrap_or("TOML"))
            .unwrap_or("TOML")
            .to_uppercase()
            .as_ref()
        {
            #[cfg(feature = "toml-config")]
            "TOML" => toml::to_string_pretty(&self.data)?,

            #[cfg(feature = "json-config")]
            "JSON" => serde_json::to_string_pretty(&self.data)?,

            ext => return Err(Error::ConfigExt(ext.to_owned()).into()),
        };

        // 2. Creating the parent directory if it does not exist.
        if let Some(parent_dir) = self.path.parent() {
            fs::create_dir_all(parent_dir).await?;
        }

        // 3. Generating a path to a temporary file (e.g., config.toml.tmp)
        let tmp_path = self.path.with_extension(format!(
            "{}.tmp",
            self.path.extension().and_then(|s| s.to_str()).unwrap_or("")
        ));

        // 4. Writing to a temporary file
        fs::write(&tmp_path, &contents).await?;

        // 5. (Optional) Forcing the OS buffers to be flushed to the physical disk (fsync)
        // To ensure that the data is physically written before renaming
        if let Ok(file) = fs::File::open(&tmp_path).await {
            let _ = file.sync_all().await;
        }

        // 6. Atomic renaming: .tmp replaces the original file
        if let Err(e) = fs::rename(&tmp_path, &self.path).await {
            // If the renaming fails, we clean up the .tmp files.
            let _ = fs::remove_file(&tmp_path).await;
            return Err(e.into());
        }

        // 7. Update the metadata regarding the time of changes in memory.
        let mut guard = self.modify.lock().await;
        guard.modified = Some(Utc::now());
        guard.checked = Some(Instant::now());

        Ok(())
    }

    /// Updates the config file
    pub async fn save(&mut self) -> Result<()> {
        self.write(self.path.clone()).await
    }

    /// Returns true if the data needs to be updated
    pub async fn check(&self, millis: u64) -> Result<bool> {
        let interval = Duration::from_millis(millis);

        // check last checked time (dirty method for quick access to the latest cached state)
        if let Some(time) = self.modify.dirty_get().checked
            && &time.elapsed() < &interval
        {
            return Ok(false);
        }

        // locking state for update the instance
        let mut guard = self.modify.lock().await;

        // check again in case the another thread is already changed instance
        if let Some(time) = guard.checked
            && &time.elapsed() < &interval
        {
            return Ok(false);
        }

        // checking the actual file metadata
        let meta = fs::metadata(&self.path).await?;
        let modified: DateTime<Utc> = meta.modified()?.into();

        if let Some(&last_modified) = self.modify.dirty_get().modified.as_ref() {
            if modified <= last_modified {
                return Ok(false);
            }
        }

        // update the last checked time
        guard.checked.replace(Instant::now());

        Ok(true)
    }

    /// Updates the struct data from config file (returns true if updated)
    pub async fn update(&mut self) -> Result<bool> {
        // read the actual file contents
        let cfg = Self::read(&self.path).await?;
        *self = cfg;

        Ok(true)
    }
}

impl<T> ::std::ops::Deref for Config<T>
where
    T: Clone + Default + Debugging + Serialize + DeserializeOwned + Send + Sync + 'static,
{
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.data
    }
}

impl<T> ::std::ops::DerefMut for Config<T>
where
    T: Clone + Default + Debugging + Serialize + DeserializeOwned + Send + Sync + 'static,
{
    fn deref_mut(&mut self) -> &mut T {
        &mut self.data
    }
}

impl<T> ::std::fmt::Debug for Config<T>
where
    T: Clone + Default + Debugging + Serialize + DeserializeOwned + Send + Sync + 'static,
{
    fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
        write!(f, "{:?}", &self.data)
    }
}

impl<T> ::std::fmt::Display for Config<T>
where
    T: Clone
        + Default
        + std::fmt::Display
        + Debugging
        + Serialize
        + DeserializeOwned
        + Send
        + Sync
        + 'static,
{
    fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
        write!(f, "{}", &self.data)
    }
}

impl<T: Clone + Default + Debugging + Serialize + DeserializeOwned + Eq + Send + Sync + 'static>
    ::std::cmp::Eq for Config<T>
{
}

impl<T> ::std::cmp::PartialEq for Config<T>
where
    T: Clone
        + Default
        + Debugging
        + Serialize
        + DeserializeOwned
        + PartialEq
        + Send
        + Sync
        + 'static,
{
    fn eq(&self, other: &Self) -> bool {
        &self.data == &other.data
    }
}

impl<T> ::std::convert::From<T> for Config<T>
where
    T: Clone + Default + Debugging + Serialize + DeserializeOwned + Send + Sync + 'static,
{
    fn from(value: T) -> Self {
        Self {
            path: Default::default(),
            data: value,
            modify: arc!(Modify::now().into()),
        }
    }
}
