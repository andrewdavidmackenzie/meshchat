use std::ops::Sub;
use std::time::{SystemTime, UNIX_EPOCH};

/// Time in EPOC in seconds timestamp
#[derive(PartialEq, PartialOrd, Ord, Eq, Debug, Default, Clone, Copy)]
pub struct TimeStamp(u128);

impl From<u128> for TimeStamp {
    fn from(value: u128) -> Self {
        TimeStamp(value)
    }
}

impl From<u64> for TimeStamp {
    fn from(value: u64) -> Self {
        TimeStamp(value as u128)
    }
}

impl From<u32> for TimeStamp {
    fn from(value: u32) -> Self {
        TimeStamp(value as u128)
    }
}

impl From<TimeStamp> for u128 {
    fn from(value: TimeStamp) -> Self {
        value.0
    }
}

impl From<TimeStamp> for u64 {
    fn from(value: TimeStamp) -> Self {
        value.0 as u64
    }
}

impl From<TimeStamp> for u32 {
    fn from(value: TimeStamp) -> Self {
        value.0 as u32
    }
}

impl From<TimeStamp> for i64 {
    fn from(value: TimeStamp) -> Self {
        value.0 as i64
    }
}

impl Sub for TimeStamp {
    type Output = TimeStamp;

    fn sub(self, rhs: Self) -> Self::Output {
        TimeStamp::from(self.0.saturating_sub(rhs.0))
    }
}

impl TimeStamp {
    /// Get the current time in epoch as u32
    pub fn now() -> Self {
        Self::from(
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis(),
        )
    }
}
