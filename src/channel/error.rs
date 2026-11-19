use macron::{Display, Error, From};

#[derive(Debug, Display, Error, From)]
pub enum ChannelError {
    #[cfg(feature = "channel")]
    #[display(fmt = "Channel is already closed.")]
    ChannelClosed,

    #[cfg(feature = "channel")]
    #[display(fmt = "Channel is overflowed.")]
    ChannelFull,
}
