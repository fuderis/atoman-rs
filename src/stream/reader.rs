use crate::prelude::*;
use tokio::sync::mpsc;

/// The stream reader
pub struct StreamReader<T> {
    pub(super) rx: Option<mpsc::UnboundedReceiver<Result<T>>>,
}

impl<T> StreamReader<T> {
    /// Reads the following T object
    pub async fn read(&mut self) -> Result<Option<T>> {
        if let Some(ref mut rx) = self.rx {
            match rx.recv().await {
                Some(Ok(item)) => Ok(Some(item)),
                Some(Err(e)) => Err(e),
                None => Ok(None),
            }
        } else {
            Ok(None)
        }
    }

    /// Tries to read following T object (without waiting)
    pub fn try_read(&mut self) -> Option<T> {
        if let Some(ref mut rx) = self.rx {
            // try_recv returns Result<T, TryRecvError>:
            match rx.try_recv() {
                Ok(Ok(item)) => Some(item),
                _ => None,
            }
        } else {
            None
        }
    }

    /// Reads the following T object
    pub async fn next(&mut self) -> Result<Option<T>> {
        self.read().await
    }

    /// Tries to read following T object (without waiting)
    pub async fn try_next(&mut self) -> Result<Option<T>> {
        self.read().await
    }

    /// Checks the channel for closed
    pub fn is_closed(&self) -> bool {
        self.rx.is_none()
    }
}

impl<T> From<mpsc::UnboundedReceiver<Result<T>>> for StreamReader<T> {
    fn from(rx: mpsc::UnboundedReceiver<Result<T>>) -> Self {
        Self { rx: Some(rx) }
    }
}
