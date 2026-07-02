use crate::prelude::*;
use tokio::sync::mpsc;

/// The channel receiver
pub enum Receiver<T> {
    Unbounded(mpsc::UnboundedReceiver<Result<T>>),
    Bounded(mpsc::Receiver<Result<T>>),
}

impl<T> Receiver<T> {
    /// Receives the next data from receiver
    pub async fn recv(&mut self) -> Result<Option<T>> {
        match self {
            Self::Unbounded(rx) => Ok(rx
                .recv()
                .await
                .map(|r| r.ok())
                .ok_or(Error::ChannelClosed)?),
            Self::Bounded(rx) => Ok(rx
                .recv()
                .await
                .map(|r| r.ok())
                .ok_or(Error::ChannelClosed)?),
        }
    }

    /// Receives the next data from receiver
    pub fn try_recv(&mut self) -> Result<Option<T>> {
        match self {
            Self::Unbounded(rx) => Ok(rx
                .try_recv()
                .map(|r| r.ok())
                .map_err(|_| Error::ChannelClosed)?),
            Self::Bounded(rx) => Ok(rx
                .try_recv()
                .map(|r| r.ok())
                .map_err(|_| Error::ChannelClosed)?),
        }
    }

    /// Checks the channel for closed
    pub fn is_closed(&self) -> bool {
        match self {
            Self::Unbounded(rx) => rx.is_closed(),
            Self::Bounded(rx) => rx.is_closed(),
        }
    }
}

impl<T> From<mpsc::UnboundedReceiver<Result<T>>> for Receiver<T> {
    fn from(rx: mpsc::UnboundedReceiver<Result<T>>) -> Self {
        Self::Unbounded(rx)
    }
}

impl<T> From<mpsc::Receiver<Result<T>>> for Receiver<T> {
    fn from(rx: mpsc::Receiver<Result<T>>) -> Self {
        Self::Bounded(rx)
    }
}
