use crate::channel_id::ChannelId::{Channel, Node};
use meshtastic::packet::PacketDestination;
use meshtastic::types::{MeshChannel, NodeId};
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

impl ChannelId {
    pub fn to_destination(&self) -> (PacketDestination, MeshChannel) {
        match self {
            Channel(channel_number) => (
                PacketDestination::Broadcast,
                MeshChannel::from(*channel_number as u32),
            ),
            Node(node_id) => (
                PacketDestination::Node(NodeId::from(*node_id)),
                MeshChannel::default(),
            ),
        }
    }
}

#[cfg(test)]
mod test {
    use crate::channel_id::ChannelId;
    use meshtastic::packet::PacketDestination;
    use meshtastic::types::MeshChannel;

    #[test]
    fn test_to_channel() {
        let channel_id = ChannelId::Channel(0);
        let (destination, channel) = channel_id.to_destination();
        match destination {
            PacketDestination::Local => panic!("Should not be local"),
            PacketDestination::Broadcast => {}
            PacketDestination::Node(_) => panic!("Should not be node"),
        };
        assert_eq!(channel, MeshChannel::from(0));
    }
}
