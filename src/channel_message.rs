use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Serialize, Deserialize)]
pub enum Payload {
    Text(String),
    Position(i32, i32),
    Ping(String), // Could add hw_model or similar if wanted
}

/// An entry in the Channel View that represents some type of message sent to either this user on
/// this device or to a channel this device can read. Can be any of [Payload] types.
#[derive(Serialize, Deserialize)]
pub struct ChannelViewEntry {
    from: u32,
    rx_time: u64,
    payload: Payload,
    seen: bool,
}

impl ChannelViewEntry {
    /// Create a new [ChannelViewEntry] from the parameters provided. The received time will be set to
    /// the current time in EPOC as an u64
    pub fn new(message: Payload, from: u32, seen: bool) -> Self {
        let rx_time = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|t| t.as_secs())
            .unwrap_or(0);

        ChannelViewEntry {
            payload: message,
            from,
            rx_time,
            seen,
        }
    }

    /// Get a reference to the payload of this message
    pub fn payload(&self) -> &Payload {
        &self.payload
    }

    pub fn from(&self) -> u32 {
        self.from
    }
}

impl PartialEq<Self> for ChannelViewEntry {
    fn eq(&self, other: &Self) -> bool {
        self.rx_time == other.rx_time
    }
}

impl PartialOrd<Self> for ChannelViewEntry {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for ChannelViewEntry {
    fn cmp(&self, other: &Self) -> Ordering {
        self.rx_time.cmp(&other.rx_time)
    }
}

impl Eq for ChannelViewEntry {}
