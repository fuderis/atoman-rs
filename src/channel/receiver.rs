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
            Self::Unbounded(rx) => match rx.recv().await {
                Some(Ok(item)) => Ok(Some(item)),
                Some(Err(err)) => Err(err),
                None => Ok(None),
            },

            Self::Bounded(rx) => match rx.recv().await {
                Some(Ok(item)) => Ok(Some(item)),
                Some(Err(err)) => Err(err),
                None => Ok(None),
            },
        }
    }

    /// Receives the next data from receiver
    pub fn try_recv(&mut self) -> Result<Option<T>> {
        use tokio::sync::mpsc::error::TryRecvError;

        match self {
            Self::Unbounded(rx) => match rx.try_recv() {
                Ok(Ok(item)) => Ok(Some(item)),
                Ok(Err(err)) => Err(err),
                Err(TryRecvError::Empty) => Ok(None),
                Err(TryRecvError::Disconnected) => Ok(None),
            },

            Self::Bounded(rx) => match rx.try_recv() {
                Ok(Ok(item)) => Ok(Some(item)),
                Ok(Err(err)) => Err(err),
                Err(TryRecvError::Empty) => Ok(None),
                Err(TryRecvError::Disconnected) => Ok(None),
            },
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
