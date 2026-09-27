use crate::prelude::*;
use tokio::sync::mpsc::{self, error::TrySendError};

/// Channel sender wrapper around bounded and unbounded tokio mpsc senders.
pub enum Sender<T> {
    Unbounded(mpsc::UnboundedSender<Result<T>>),
    Bounded(mpsc::Sender<Result<T>>),
}

impl<T> Clone for Sender<T> {
    fn clone(&self) -> Self {
        match self {
            Self::Unbounded(tx) => Self::Unbounded(tx.clone()),
            Self::Bounded(tx) => Self::Bounded(tx.clone()),
        }
    }
}

impl<T> Sender<T> {
    /// Helper method to send raw Result through synchronous try_send.
    fn send_raw_sync(&self, res: Result<T>) -> Result<()> {
        match self {
            Self::Unbounded(tx) => tx.send(res).map_err(|_| Error::ChannelClosed.into()),
            Self::Bounded(tx) => match tx.try_send(res) {
                Ok(_) => Ok(()),
                Err(TrySendError::Full(_)) => Err(Error::ChannelFull.into()),
                Err(TrySendError::Closed(_)) => Err(Error::ChannelClosed.into()),
            },
        }
    }

    /// Helper method to send raw Result asynchronously.
    async fn send_raw_async(&self, res: Result<T>) -> Result<()> {
        match self {
            Self::Unbounded(tx) => tx.send(res).map_err(|_| Error::ChannelClosed.into()),
            Self::Bounded(tx) => tx.send(res).await.map_err(|_| Error::ChannelClosed.into()),
        }
    }

    /// Non-blocking send: returns ChannelFull error if bounded channel is capacity-constrained.
    pub fn send(&self, item: impl Into<T>) -> Result<()> {
        self.send_raw_sync(Ok(item.into()))
    }

    /// Async send: waits until capacity is available in bounded channel.
    pub async fn send_async(&self, item: impl Into<T>) -> Result<()> {
        self.send_raw_async(Ok(item.into())).await
    }

    /// Non-blocking error send.
    pub fn send_err(&self, error: DynError) -> Result<()> {
        self.send_raw_sync(Err(error))
    }

    /// Async error send.
    pub async fn send_err_async(&self, error: DynError) -> Result<()> {
        self.send_raw_async(Err(error)).await
    }

    /// Checks if the channel is closed.
    pub fn is_closed(&self) -> bool {
        match self {
            Self::Unbounded(tx) => tx.is_closed(),
            Self::Bounded(tx) => tx.is_closed(),
        }
    }

    /// Waits until the receiver is closed.
    pub async fn closed(&self) {
        match self {
            Self::Unbounded(tx) => tx.closed().await,
            Self::Bounded(tx) => tx.closed().await,
        }
    }
}

impl<T> From<mpsc::UnboundedSender<Result<T>>> for Sender<T> {
    fn from(tx: mpsc::UnboundedSender<Result<T>>) -> Self {
        Self::Unbounded(tx)
    }
}

impl<T> From<mpsc::Sender<Result<T>>> for Sender<T> {
    fn from(tx: mpsc::Sender<Result<T>>) -> Self {
        Self::Bounded(tx)
    }
}
