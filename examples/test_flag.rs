use atoman::prelude::*;

static IS_ACTIVE: Flag = Flag::new();

#[tokio::main]
async fn main() {
    assert!(!IS_ACTIVE.is_locked());

    IS_ACTIVE.lock().await;
    assert!(IS_ACTIVE.is_locked());

    IS_ACTIVE.unlock();
    assert!(!IS_ACTIVE.is_locked());

    IS_ACTIVE.blocking_lock();
    assert!(IS_ACTIVE.is_locked());
}
