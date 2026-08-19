use crate::prelude::*;
use tokio::sync::mpsc::{self, error::TrySendError};

/// The channel sender
#[derive(Clone)]
pub enum Sender<T> {
    Unbounded(mpsc::UnboundedSender<Result<T>>),
    Bounded(mpsc::Sender<Result<T>>),
}

impl<T> Sender<T> {
    /// Sends a data to the receiver
    pub fn send(&self, item: impl Into<T>) -> Result<()> {
        match self {
            Self::Unbounded(tx) => tx
                .send(Ok(item.into()))
                .map_err(|_| Error::ChannelClosed.into()),
            Self::Bounded(tx) => match tx.try_send(Ok(item.into())) {
                Ok(_) => Ok(()),
                Err(TrySendError::Full(_)) => Err(Error::ChannelFull.into()),
                Err(TrySendError::Closed(_)) => Err(Error::ChannelClosed.into()),
            },
        }
    }

    /// Tries to a send data to the receiver
    pub async fn send_async(&self, item: impl Into<T>) -> Result<()> {
        match self {
            Self::Unbounded(tx) => tx
                .send(Ok(item.into()))
                .map_err(|_| Error::ChannelClosed.into()),
            Self::Bounded(tx) => tx
                .send(Ok(item.into()))
                .await
                .map_err(|_| Error::ChannelClosed.into()),
        }
    }

    /// Sends an error to receiver
    pub fn send_err(&self, error: DynError) -> Result<()> {
        match self {
            Self::Unbounded(tx) => tx.send(Err(error)).map_err(|_| Error::ChannelClosed.into()),

            Self::Bounded(tx) => match tx.try_send(Err(error)) {
                Ok(_) => Ok(()),
                Err(TrySendError::Full(_)) => Err(Error::ChannelFull.into()),
                Err(TrySendError::Closed(_)) => Err(Error::ChannelClosed.into()),
            },
        }
    }

    /// Tries to send an error to receiver
    pub async fn send_err_async(&self, error: DynError) -> Result<()> {
        match self {
            Self::Unbounded(tx) => tx.send(Err(error)).map_err(|_| Error::ChannelClosed.into()),

            Self::Bounded(tx) => tx
                .send(Err(error))
                .await
                .map_err(|_| Error::ChannelClosed.into()),
        }
    }

    /// Checks the channel for closed
    pub fn is_closed(&self) -> bool {
        match self {
            Self::Unbounded(tx) => tx.is_closed(),
            Self::Bounded(tx) => tx.is_closed(),
        }
    }

    /// Works until the connection is closed
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
