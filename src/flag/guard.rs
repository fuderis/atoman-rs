use super::Flag;

/// Atomic flag guard.
pub struct FlagGuard<'a> {
    pub(super) flag: &'a Flag,
}

/// Disables flag on drop (set to false).
impl Drop for FlagGuard<'_> {
    fn drop(&mut self) {
        self.flag.disable();
    }
}
