#![cfg(all(feature = "trace", feature = "logger"))]
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
