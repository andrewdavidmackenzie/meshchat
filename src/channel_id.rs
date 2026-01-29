use crate::channel_id::ChannelId::Channel;
use serde::{Deserialize, Serialize};
use std::fmt::{Display, Formatter};
use std::hash::Hash;

#[derive(Debug, Clone, Serialize, Deserialize, Hash, PartialEq, Eq)]
pub enum ChannelId {
    Channel(i32), // Channel::index 0..7
    Node(u32),    // NodeInfo::node number
}

impl Default for ChannelId {
    fn default() -> Self {
        Channel(0)
    }
}

impl Display for ChannelId {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "{:?}", self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_is_channel_zero() {
        let channel_id = ChannelId::default();
        assert_eq!(channel_id, ChannelId::Channel(0));
    }

    #[test]
    fn test_channel_equality() {
        let ch1 = ChannelId::Channel(1);
        let ch2 = ChannelId::Channel(1);
        let ch3 = ChannelId::Channel(2);
        assert_eq!(ch1, ch2);
        assert_ne!(ch1, ch3);
    }

    #[test]
    fn test_node_equality() {
        let node1 = ChannelId::Node(12345);
        let node2 = ChannelId::Node(12345);
        let node3 = ChannelId::Node(67890);
        assert_eq!(node1, node2);
        assert_ne!(node1, node3);
    }

    #[test]
    fn test_channel_and_node_not_equal() {
        let channel = ChannelId::Channel(1);
        let node = ChannelId::Node(1);
        assert_ne!(channel, node);
    }

    #[test]
    fn test_display_channel() {
        let channel = ChannelId::Channel(5);
        let display = format!("{}", channel);
        assert!(display.contains("Channel"));
        assert!(display.contains("5"));
    }

    #[test]
    fn test_display_node() {
        let node = ChannelId::Node(12345);
        let display = format!("{}", node);
        assert!(display.contains("Node"));
        assert!(display.contains("12345"));
    }

    #[test]
    fn test_clone() {
        let original = ChannelId::Channel(3);
        let cloned = original.clone();
        assert_eq!(original, cloned);
    }

    #[test]
    fn test_hash_consistency() {
        use std::collections::HashMap;

        let mut map: HashMap<ChannelId, &str> = HashMap::new();
        map.insert(ChannelId::Channel(0), "channel_0");
        map.insert(ChannelId::Node(100), "node_100");

        assert_eq!(map.get(&ChannelId::Channel(0)), Some(&"channel_0"));
        assert_eq!(map.get(&ChannelId::Node(100)), Some(&"node_100"));
        assert_eq!(map.get(&ChannelId::Channel(1)), None);
    }

    #[test]
    fn test_debug_format() {
        let channel = ChannelId::Channel(2);
        let debug = format!("{:?}", channel);
        assert_eq!(debug, "Channel(2)");

        let node = ChannelId::Node(999);
        let debug = format!("{:?}", node);
        assert_eq!(debug, "Node(999)");
    }
}
