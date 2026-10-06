use atoman::State;

static CONFIG: State<Config> = State::new(|| Config { count: 10 });

#[derive(Default, Clone)]
pub struct Config {
    pub count: i32,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    assert_eq!(CONFIG.get().count, 10);

    CONFIG.set(Config { count: 15 }).await;
    assert_eq!(CONFIG.get().count, 15);

    CONFIG.lock().await.count = 20;
    assert_eq!(CONFIG.get().count, 20);

    Ok(())
}
