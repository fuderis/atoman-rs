pub mod guard;
pub use guard::FlagGuard;

use std::{
    ops::Deref,
    sync::atomic::{AtomicBool, Ordering},
};

/// Light-weight atomic flag.
#[derive(Debug)]
pub struct Flag {
    state: AtomicBool,
}

impl Flag {
    /// Creates new atomic flag.
    pub const fn new(initial: bool) -> Self {
        Self {
            state: AtomicBool::new(initial),
        }
    }

    /// Enables flag (sets to true).
    #[inline]
    pub fn enable(&self) -> bool {
        self.state.swap(true, Ordering::AcqRel)
    }

    /// Disables flag (sets to false).
    #[inline]
    pub fn disable(&self) -> bool {
        self.state.swap(false, Ordering::AcqRel)
    }

    /// Toggles flag (from true to false, or vice versa).
    #[inline]
    pub fn toggle(&self) -> bool {
        self.state.fetch_xor(true, Ordering::AcqRel)
    }

    /// Sets flag boolean (manual).
    #[inline]
    pub fn set(&self, value: bool) {
        self.state.store(value, Ordering::Release);
    }

    /// Returns true if enabled.
    #[inline]
    pub fn is_enabled(&self) -> bool {
        self.state.load(Ordering::Acquire)
    }

    /// Returns true if disabled.
    #[inline]
    pub fn is_disabled(&self) -> bool {
        !self.is_enabled()
    }

    /// Enables flag and disables on drop.
    pub fn guard(&self) -> FlagGuard<'_> {
        self.enable();
        FlagGuard { flag: self }
    }
}

impl Deref for Flag {
    type Target = bool;

    #[inline]
    fn deref(&self) -> &Self::Target {
        if self.is_enabled() { &true } else { &false }
    }
}

impl Eq for Flag {}

impl PartialEq for Flag {
    #[inline]
    fn eq(&self, other: &Self) -> bool {
        self.is_enabled() == other.is_enabled()
    }
}

impl PartialEq<bool> for Flag {
    #[inline]
    fn eq(&self, other: &bool) -> bool {
        self.is_enabled() == *other
    }
}

impl PartialEq<Flag> for bool {
    #[inline]
    fn eq(&self, other: &Flag) -> bool {
        *self == other.is_enabled()
    }
}

impl PartialOrd for Flag {
    #[inline]
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        self.is_enabled().partial_cmp(&other.is_enabled())
    }
}

impl PartialOrd<bool> for Flag {
    #[inline]
    fn partial_cmp(&self, other: &bool) -> Option<std::cmp::Ordering> {
        self.is_enabled().partial_cmp(other)
    }
}

impl std::fmt::Display for Flag {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.is_enabled())
    }
}

impl Default for Flag {
    fn default() -> Self {
        Self::new(false)
    }
}

impl From<bool> for Flag {
    fn from(value: bool) -> Self {
        Self::new(value)
    }
}

impl From<&Flag> for bool {
    fn from(flag: &Flag) -> Self {
        flag.is_enabled()
    }
}
