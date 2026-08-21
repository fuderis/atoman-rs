use atoman::Map;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::time::{Duration, Instant, sleep};

#[derive(Debug)]
pub struct UserSession {
    pub token: String,
    pub last_active: u64,
    pub actions_count: u64,
}

impl UserSession {
    pub fn new(token: impl Into<String>) -> Self {
        Self {
            token: token.into(),
            last_active: Self::now(),
            actions_count: 0,
        }
    }

    fn now() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs()
    }
}

/// Глобальное хранилище сессий без `Lazy` обёрток
static SESSIONS: Map<u64, UserSession> = Map::new();

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    const SESSIONS_COUNT: u64 = 100;

    println!("⚡ Инициализация {} активных сессий...", SESSIONS_COUNT);

    // 1. Создаём сессии для пользователей
    for user_id in 0..SESSIONS_COUNT {
        SESSIONS.insert(user_id, UserSession::new(format!("token_sess_{user_id}")));
    }

    let start_time = Instant::now();
    println!("🚀 Эмуляция параллельной активности пользователей...");

    // 2. Эмулируем 100 одновременно пришедших запросов
    let mut tasks = Vec::with_capacity(SESSIONS_COUNT as usize);

    for user_id in 0..SESSIONS_COUNT {
        let task = tokio::spawn(async move {
            // Обновляем сессию конкретного пользователя
            if let Some(mut session) = SESSIONS.write(&user_id).await {
                // Имитируем задержку обработки запроса (10 мс)
                sleep(Duration::from_millis(10)).await;

                session.actions_count += 1;
                session.last_active = UserSession::now();
            }
        });
        tasks.push(task);
    }

    // 3. Ждём завершения всех асинхронных тасок
    for task in tasks {
        task.await?;
    }

    let elapsed = start_time.elapsed();
    println!("✅ Все сессии обновлены за: {:?}", elapsed);

    // 4. Валидация: проверяем, что все сессии корректно обновились
    for user_id in 0..SESSIONS_COUNT {
        let session = SESSIONS
            .read(&user_id)
            .await
            .expect("Сессия должна существовать");
        assert_eq!(session.actions_count, 1);
        assert_eq!(session.token, format!("token_sess_{user_id}"));
    }

    println!("🎉 Проверка пройдена: сессии абсолютно независимы и не блокируют друг друга!");
    Ok(())
}
