use atoman::State;

static CONFIG: State<Config> = State::new(|| Config { count: 10 });

#[derive(Default, Clone)]
pub struct Config {
    pub count: i32,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    #[cfg(feature = "trace-lock")]
    atoman::Logger::init("", 0).await?;
    #[cfg(feature = "trace-lock")]
    atoman::Logger::set_level(atoman::Level::TRACE).await;

    assert_eq!(CONFIG.get().await.count, 10);

    CONFIG.blocking_set(Config { count: 15 });
    assert_eq!(CONFIG.blocking_get().count, 15);

    CONFIG.dirty_set(Config { count: 20 });
    assert_eq!(CONFIG.dirty_get().count, 20);

    CONFIG.lock().await.count = 30;
    assert_eq!(CONFIG.get().await.count, 30);

    #[cfg(feature = "trace-lock")]
    atoman::Logger::flush().await;
    Ok(())
}
