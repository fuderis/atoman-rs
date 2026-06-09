[![github]](https://github.com/fuderis/atoman-rs)&ensp;
[![crates-io]](https://crates.io/crates/atoman)&ensp;
[![docs-rs]](https://docs.rs/atoman)

[github]: https://img.shields.io/badge/github-8da0cb?style=for-the-badge&labelColor=555555&logo=github
[crates-io]: https://img.shields.io/badge/crates.io-fc8d62?style=for-the-badge&labelColor=555555&logo=rust
[docs-rs]: https://img.shields.io/badge/docs.rs-66c2a5?style=for-the-badge&labelColor=555555&logo=docs.rs

# Atoman (atomic state, flag and more..)

Atoman is a Rust library for safe concurrent access to static asynchronous data across your application. 
It provides atomic flags and state wrappers with async setters, getters, and locking mechanisms, 
bridging synchronous static variables with async runtimes like Tokio.<br>

It provides blocking methods alongside async APIs, enabling seamless use in both asynchronous (Tokio tasks) 
and synchronous (threads/blocking code) contexts without deadlocks or runtime conflicts.

## Features:

* Global configuration management with file-based persistence (`TOML/JSON`).
* Structured logging with async-safe output to files.
* Real-time log file tracing using minimal memory stack.
* Feature flags and shared state in async applications.
* Data is stored in `Arc` for zero-copy access and thread safety.

## Examples:

### Atomic Flag:
```rust
use atoman::prelude::*;

static IS_ACTIVE: Flag = Flag::new();

#[tokio::main]
async fn main() {
    assert!(!IS_ACTIVE.get());
    assert!(IS_ACTIVE.is_false());

    IS_ACTIVE.set(true);
    assert!(IS_ACTIVE.get());

    IS_ACTIVE.swap(false).await;
    assert!(!IS_ACTIVE.get());

    IS_ACTIVE.blocking_swap(true);
    assert!(IS_ACTIVE.get());
}
```

### Atomic State:
```rust
use atoman::prelude::*;

static CONFIG: State<Config> = State::new();

#[derive(Default, Clone)]
pub struct Config {
    pub count: i32,
}

#[tokio::main]
async fn main() {
    CONFIG.set(Config { count: 10, }).await;
    assert_eq!(CONFIG.get().await.count, 10);

    CONFIG.blocking_set(Config { count: 15 });
    assert_eq!(CONFIG.blocking_get().count, 15);

    CONFIG.map(|cfg| cfg.count = 20).await;
    assert_eq!(CONFIG.get().await.count, 20);
    
    CONFIG.lock().await.count = 30;
    assert_eq!(CONFIG.get().await.count, 30);
}
```

### Config (feature `config`):
```rust
use atoman::prelude::*;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct Person {
    name: String,
    age: u32
}

impl Default for Person {
    fn default() -> Self {
        Self {
            name: "Bob".to_owned(),
            age: 23
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let mut cfg = Config::<Person>::new(".test/person.toml").await?;
    
    assert_eq!(cfg.name, "Bob");
    assert_eq!(cfg.age, 23);

    cfg.age = 24;
    assert_eq!(cfg.age, 24);

    Ok(())
}
```

### Logger (feature `logger`):
```rust
use atoman::prelude::*;

#[tokio::main]
async fn main() -> Result<()> {
    Logger::init(".logs", 1000).await?;
        
    info!("Hello, World!");

    Ok(())
}
```

### Tracing (feature `trace`):
```rust
use atoman::{Logger, Trace, info, log};
use tokio::time::{Duration, sleep};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // init logger:
    Logger::init(".logs", 1000).await?;
    info!("Logger initialized!");

    // get log file path:
    Logger::flush().await;
    let log_path = Logger::path().unwrap();

    // start log file tracing:
    let trace_handle = tokio::spawn(async move {
        info!("Tracing file: {}", log_path.display());

        let trace = Trace::open(
            log_path,
            Duration::from_millis(50),
            vec!["uid=342".to_string()],
            true,
        )
        .await
        .expect("Failed to open trace");

        // read next lines:
        let mut count = 0;
        while count < 5 {
            if let Some(lines) = trace.read().await {
                for line in lines {
                    println!("Traced line: {line}");
                }
                count += 1;
            }
        }

        // fast check (without blocking thread):
        for _ in 0..5 {
            if let Some(lines) = trace.try_read().await {
                for line in lines {
                    println!("Traced line: {line}");
                }
            }
            tokio::time::sleep(Duration::from_millis(120)).await;
        }
    });

    // wait for spawn thread:
    sleep(Duration::from_millis(100)).await;

    for i in 1..=15 {
        handle(if i % 2 == 0 { 342 } else { 741 }, i);
        sleep(Duration::from_millis(100)).await;
    }

    // waiting trace thread:
    let _ = trace_handle.await;

    Ok(())
}

#[log(skip(cicle))]
fn handle(uid: u64, cicle: usize) {
    info!("Test log {cicle}...");
}
```

### Stream (feature `stream`):
```rust
use atoman::{Stream, StreamExt, Bytes};

/// Handles server page (on Axum framework as example)
pub async fn test_page(
    Json(data): Json<JsonValue>,
) -> impl IntoResponse {
    let body = Stream::body(
        // spawn page handler:
        move |tx| async move {
            for i in 0..5 {
                tx.send(Bytes::from(i.to_string())).ok()
            }
        }
    )
    .await;

    (
        StatusCode::OK,
        HeaderMap::from_iter(map!{
            header::CONTENT_TYPE =>
            "application/octet-stream".parse().unwrap(),
        }),
        Body::from_stream(body),
    )
        .into_response()
}
```

## License & Feedback:

> This library distributed under the [MIT](https://github.com/fuderis/atoman-rs/blob/main/LICENSE.md) license.

You can contact me via [GitHub](https://github.com/fuderis) or send a message to my [E-Mail](mailto:synapdrake@ya.ru).
This library is actively evolving, and your suggestions and feedback are always welcome!
