use crate::channel_id::ChannelId::Channel;
use serde::{Deserialize, Serialize};
use std::fmt::{Display, Formatter};
use std::hash::Hash;

#[derive(Debug, Default, Serialize, Deserialize, Hash, PartialEq, Eq, Clone, Copy)]
pub struct NodeId(u64);

impl From<u64> for NodeId {
    fn from(value: u64) -> Self {
        NodeId(value)
    }
}

impl From<NodeId> for u64 {
    fn from(value: NodeId) -> Self {
        value.0
    }
}

impl From<u32> for NodeId {
    fn from(value: u32) -> Self {
        NodeId(value as u64)
    }
}

impl From<NodeId> for u32 {
    fn from(value: NodeId) -> Self {
        value.0 as u32
    }
}

impl From<&NodeId> for u64 {
    fn from(value: &NodeId) -> Self {
        value.0
    }
}

impl Display for NodeId {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(&format!("{}", self.0))
    }
}

#[derive(Debug, Default, Hash, PartialEq, Eq, Clone, Copy)]
pub struct MessageId(u32);

impl From<u32> for MessageId {
    fn from(value: u32) -> Self {
        MessageId(value)
    }
}

impl From<MessageId> for u32 {
    fn from(value: MessageId) -> Self {
        value.0
    }
}

impl Display for MessageId {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(&format!("{}", self.0))
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Hash, PartialEq, Eq)]
pub enum ChannelId {
    Channel(i32), // Channel::index 0..7
    Node(NodeId), // NodeInfo::node number
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
    use crate::channel_id::ChannelId::Node;

    #[test]
    fn test_default_is_channel_zero() {
        let channel_id = ChannelId::default();
        assert_eq!(channel_id, Channel(0));
    }

    #[test]
    fn test_channel_equality() {
        let ch1 = Channel(1);
        let ch2 = Channel(1);
        let ch3 = Channel(2);
        assert_eq!(ch1, ch2);
        assert_ne!(ch1, ch3);
    }

    #[test]
    fn test_node_equality() {
        let node1 = Node(NodeId(12345));
        let node2 = Node(NodeId(12345));
        let node3 = Node(NodeId(67890));
        assert_eq!(node1, node2);
        assert_ne!(node1, node3);
    }

    #[test]
    fn test_channel_and_node_not_equal() {
        let channel = Channel(1);
        let node = Node(NodeId(1));
        assert_ne!(channel, node);
    }

    #[test]
    fn test_display_channel() {
        let channel = Channel(5);
        let display = format!("{}", channel);
        assert!(display.contains("Channel"));
        assert!(display.contains("5"));
    }

    #[test]
    fn test_display_node() {
        let node = Node(NodeId(12345));
        let display = format!("{}", node);
        assert!(display.contains("Node"));
        assert!(display.contains("12345"));
    }

    #[test]
    fn test_clone() {
        let original = Channel(3);
        let cloned = original;
        assert_eq!(original, cloned);
    }

    #[test]
    fn test_hash_consistency() {
        use std::collections::HashMap;

        let mut map: HashMap<ChannelId, &str> = HashMap::new();
        map.insert(Channel(0), "channel_0");
        map.insert(Node(NodeId(100)), "node_100");

        assert_eq!(map.get(&Channel(0)), Some(&"channel_0"));
        assert_eq!(map.get(&Node(NodeId(100))), Some(&"node_100"));
        assert_eq!(map.get(&Channel(1)), None);
    }

    #[test]
    fn test_debug_format() {
        let channel = Channel(2);
        let debug = format!("{:?}", channel);
        assert_eq!(debug, "Channel(2)");

        let node = Node(NodeId(999));
        let debug = format!("{:?}", node);
        assert_eq!(debug, "Node(NodeId(999))");
    }
}
