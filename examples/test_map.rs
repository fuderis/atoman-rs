use atoman::SharedMap;
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

/// Global session storage without `Lazy` wrappers
static SESSIONS: SharedMap<u64, UserSession> = SharedMap::new();

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    const SESSIONS_COUNT: u64 = 100;

    println!("Initializing {} active sessions...", SESSIONS_COUNT);

    // 1. Create sessions for users
    for user_id in 0..SESSIONS_COUNT {
        SESSIONS
            .insert(user_id, UserSession::new(format!("token_sess_{user_id}")))
            .await;
    }

    let start_time = Instant::now();
    println!("Simulating concurrent user activity...");

    // 2. Simulate 100 incoming requests simultaneously
    let mut tasks = Vec::with_capacity(SESSIONS_COUNT as usize);

    for user_id in 0..SESSIONS_COUNT {
        let task = tokio::spawn(async move {
            // Update session for a specific user
            if let Some(mut session) = SESSIONS.write(&user_id).await {
                // Simulate request processing delay (10 ms)
                sleep(Duration::from_millis(10)).await;

                session.actions_count += 1;
                session.last_active = UserSession::now();
            }
        });
        tasks.push(task);
    }

    // 3. Wait for all asynchronous tasks to complete
    for task in tasks {
        task.await?;
    }

    let elapsed = start_time.elapsed();
    println!("All sessions updated in: {:?}", elapsed);

    // 4. Validation: check that all sessions were updated correctly
    for user_id in 0..SESSIONS_COUNT {
        let session = SESSIONS.read(&user_id).await.expect("Session should exist");
        assert_eq!(session.actions_count, 1);
        assert_eq!(session.token, format!("token_sess_{user_id}"));
    }

    println!("Check passed: sessions are completely independent and do not block each other!");
    Ok(())
}
