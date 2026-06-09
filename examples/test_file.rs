#![cfg(feature = "file")]
use atoman::File;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let mut file = File::open_read_write("test.md");

    Ok(())
}
