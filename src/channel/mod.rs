pub mod sender;
pub use sender::Sender;

pub mod receiver;
pub use receiver::Receiver;

use tokio::sync::{mpsc, oneshot};

/// Creates bounded or unbounded channel.
pub fn channel<T>(capacity: Option<usize>) -> (Sender<T>, Receiver<T>) {
    if let Some(capacity) = capacity {
        bounded_channel(capacity)
    } else {
        unbounded_channel()
    }
}

/// Creates unbounded channel.
pub fn unbounded_channel<T>() -> (Sender<T>, Receiver<T>) {
    let (tx, rx) = mpsc::unbounded_channel();
    (Sender::from(tx), Receiver::from(rx))
}

/// Creates bounded channel.
pub fn bounded_channel<T>(capacity: usize) -> (Sender<T>, Receiver<T>) {
    let (tx, rx) = mpsc::channel(capacity);
    (Sender::from(tx), Receiver::from(rx))
}

/// Creates oneshot channel.
pub fn oneshot_channel<T>() -> (Sender<T>, Receiver<T>) {
    let (tx, rx) = oneshot::channel();
    (Sender::from(tx), Receiver::from(rx))
}
