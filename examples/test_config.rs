#![cfg(feature = "config")]
use serde::{Deserialize, Serialize};

#[atoman::config]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Options {
    pub name: String,
    pub age: u32,
}

impl ::std::default::Default for Options {
    fn default() -> Self {
        Self {
            name: "Bob".to_owned(),
            age: 23,
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // initialize the global config using the auto‑generated init() method.
    Options::init("options.toml").await?;

    // printing the data
    println!("{:?}", Options::get());

    // access to fields via Deref from Arc<Config<T>>
    assert_eq!(Options::get().name, "Bob");
    assert_eq!(Options::get().age, 23);

    // modify the data using the asynchronous StateGuard lock.
    {
        let mut cfg = Options::lock().await;
        cfg.age = 24;
        cfg.save().await?; // write it atomically to the disk.
    }

    // checking the updated value
    assert_eq!(Options::get().age, 24);

    // get the current path to the configuration file.
    println!("Config path: {:?}", Options::path());

    tokio::fs::remove_file("options.toml").await?;
    Ok(())
}
