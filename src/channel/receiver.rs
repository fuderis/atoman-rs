use crate::prelude::*;
use tokio::sync::{mpsc, oneshot};

/// Generic channel receiver.
pub enum Receiver<T> {
    Unbounded(mpsc::UnboundedReceiver<Result<T>>),
    Bounded(mpsc::Receiver<Result<T>>),
    Oneshot(Option<oneshot::Receiver<Result<T>>>),
}

impl<T> Receiver<T> {
    /// Receives next data from receiver.
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

            Self::Oneshot(rx_opt) => {
                let Some(rx) = rx_opt.take() else {
                    return Ok(None);
                };
                match rx.await {
                    Ok(Ok(item)) => Ok(Some(item)),
                    Ok(Err(err)) => Err(err),
                    Err(_) => Ok(None), // Disconnected / closed
                }
            }
        }
    }

    /// Non-blocking attempt to receive data from receiver.
    pub fn try_recv(&mut self) -> Result<Option<T>> {
        use mpsc::error::TryRecvError as MpscTryRecvError;
        use oneshot::error::TryRecvError as OneshotTryRecvError;

        match self {
            Self::Unbounded(rx) => match rx.try_recv() {
                Ok(Ok(item)) => Ok(Some(item)),
                Ok(Err(err)) => Err(err),
                Err(MpscTryRecvError::Empty | MpscTryRecvError::Disconnected) => Ok(None),
            },

            Self::Bounded(rx) => match rx.try_recv() {
                Ok(Ok(item)) => Ok(Some(item)),
                Ok(Err(err)) => Err(err),
                Err(MpscTryRecvError::Empty | MpscTryRecvError::Disconnected) => Ok(None),
            },

            Self::Oneshot(rx_opt) => {
                let Some(mut rx) = rx_opt.take() else {
                    return Ok(None);
                };
                match rx.try_recv() {
                    Ok(Ok(item)) => Ok(Some(item)),
                    Ok(Err(err)) => Err(err),
                    Err(OneshotTryRecvError::Empty) => {
                        // Возвращаем обратно в Option, так как сигнал еще не пришел
                        *rx_opt = Some(rx);
                        Ok(None)
                    }
                    Err(OneshotTryRecvError::Closed) => Ok(None),
                }
            }
        }
    }

    /// Checks channel for closed.
    pub fn is_closed(&self) -> bool {
        match self {
            Self::Unbounded(rx) => rx.is_closed(),
            Self::Bounded(rx) => rx.is_closed(),
            Self::Oneshot(rx_opt) => rx_opt.is_none(),
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

impl<T> From<oneshot::Receiver<Result<T>>> for Receiver<T> {
    fn from(rx: oneshot::Receiver<Result<T>>) -> Self {
        Self::Oneshot(Some(rx))
    }
}
