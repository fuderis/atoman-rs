#![cfg(all(feature = "trace", feature = "logger"))]

use atoman::{
    Logger, info, log,
    time::{Duration, sleep},
    trace::{Filters, Source, Trace},
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // init logger for test
    Logger::init(".logs", 1000).await?;
    info!("Logger initialized!");

    // flush and resolve log directory
    Logger::flush().await;
    let log_path = Logger::path().expect("Logger path should be initialized");
    let log_dir = log_path
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| ".logs".into());

    info!(
        "Tracing directory: {} (file: {})",
        log_dir.display(),
        log_path.display()
    );

    // configure source and filters
    let sources = vec![Source::new("TRACED", ".logs", true, true)];
    let filters = Filters::new(&[], &[("uid", "342")]);

    // start tracing with handler
    let _tracer = Trace::trace(sources, Duration::from_millis(50), filters).await;

    // let background watcher initialize
    sleep(Duration::from_millis(100)).await;

    for i in 1..=15 {
        handle(if i % 2 == 0 { 342 } else { 741 }, i);
        Logger::flush().await;
        sleep(Duration::from_millis(100)).await;
    }

    // give trace workers time to flush remaining entries
    sleep(Duration::from_millis(250)).await;

    Ok(())
}

#[log(uid = %uid)]
fn handle(uid: u64, cicle: usize) {
    info!("Test log {cicle}...");
}
