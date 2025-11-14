use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Serialize, Deserialize)]
pub enum Payload {
    TextMessage(String),
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
    name: Option<String>,
    seen: bool,
}

impl ChannelViewEntry {
    /// Create a new [ChannelViewEntry] from the parameters provided. The received time will be set to
    /// the current time in EPOC as an u64
    pub fn new(message: Payload, from: u32, name: Option<String>, seen: bool) -> Self {
        let rx_time = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|t| t.as_secs())
            .unwrap_or(0);

        ChannelViewEntry {
            payload: message,
            from,
            rx_time,
            name,
            seen,
        }
    }

    /// Get a reference to the payload of this message
    pub fn payload(&self) -> &Payload {
        &self.payload
    }

    /// Return true if this message was sent from the specified node id
    pub fn source_node(&self, node_id: u32) -> bool {
        self.from == node_id
    }

    /// Return the time this message was received/sent as u64 seconds in EPOCH time
    pub fn time(&self) -> u64 {
        self.rx_time
    }

    /// Return the optional name of the sender of this messag
    pub fn name(&self) -> &Option<String> {
        &self.name
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
