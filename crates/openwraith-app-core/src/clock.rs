//! The worker's clock: the system clock, or the simulator's virtual clock
//! (so that tests run every timing rule without sleeping).

use std::time::Instant;

/// A source of the current time.
pub(crate) trait Clock: Send + Sync {
    /// Now.
    fn now(&self) -> Instant;
}

/// The system clock.
#[derive(Debug, Clone, Copy, Default)]
#[cfg_attr(feature = "sim", allow(dead_code))]
pub(crate) struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> Instant {
        Instant::now()
    }
}

/// Whole milliseconds, saturating at `u32::MAX`.
pub(crate) fn millis(duration: std::time::Duration) -> u32 {
    u32::try_from(duration.as_millis()).unwrap_or(u32::MAX)
}
