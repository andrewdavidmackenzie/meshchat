use serde::{Deserialize, Serialize};
use std::cmp::Ordering;

#[derive(Serialize, Deserialize)]
pub enum ChannelMsg {
    Text(String),
    Position(i32, i32),
    Ping(String), // Could add hw_model or similar if wanted
}

// A text message to this user on this device, sent from another device
#[derive(Serialize, Deserialize)]
pub struct ChannelMessage {
    // TODO see if we can/should make some of these private with methods
    pub from: u32,
    pub rx_time: u64,
    pub message: ChannelMsg,
    pub seen: bool,
}

impl PartialEq<Self> for ChannelMessage {
    fn eq(&self, other: &Self) -> bool {
        self.rx_time == other.rx_time
    }
}

impl PartialOrd<Self> for ChannelMessage {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for ChannelMessage {
    fn cmp(&self, other: &Self) -> Ordering {
        self.rx_time.cmp(&other.rx_time)
    }
}

impl Eq for ChannelMessage {}
