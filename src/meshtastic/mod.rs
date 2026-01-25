use crate::channel_id::ChannelId;
use crate::channel_id::ChannelId::{Channel, Node};
use crate::{MCChannel, MCNodeInfo, MCPosition, MCUser};
use meshtastic::packet::PacketDestination;
use meshtastic::protobufs::{NodeInfo, Position, User};
use meshtastic::types::{MeshChannel, NodeId};

pub mod device_subscription;

/// Conversions between [User] and MeshChat [MCUser]
impl From<&User> for MCUser {
    fn from(user: &User) -> Self {
        MCUser {
            id: user.id.clone(),
            long_name: user.long_name.clone(),
            short_name: user.short_name.clone(),
            hw_model: user.hw_model().as_str_name().to_string(),
            is_licensed: user.is_licensed,
            role: user.role().as_str_name().to_string(),
            public_key: user.public_key.clone(),
            is_unmessagable: user.is_unmessagable.unwrap_or(false),
        }
    }
}

/// Conversions between [NodeInfo] and MeshChat [MCNodeInfo]
impl From<&NodeInfo> for MCNodeInfo {
    fn from(node_info: &NodeInfo) -> Self {
        MCNodeInfo {
            num: node_info.num,
            user: node_info.user.as_ref().map(|u| u.into()),
            position: node_info.position.as_ref().map(|p| p.into()),
            channel: node_info.channel,
            is_ignored: node_info.is_ignored,
        }
    }
}

/// Conversions between [Position] and MeshChat [MCPosition]
impl From<&Position> for MCPosition {
    fn from(position: &Position) -> Self {
        MCPosition {
            latitude_i: position.latitude_i.unwrap_or(0),
            longitude_i: position.longitude_i.unwrap_or(0),
            timestamp: position.timestamp,
        }
    }
}

impl From<&meshtastic::protobufs::Channel> for MCChannel {
    fn from(channel: &meshtastic::protobufs::Channel) -> Self {
        let name = match channel.settings {
            Some(ref settings) => {
                if settings.name.is_empty() {
                    "Default".to_string()
                } else {
                    settings.name.clone()
                }
            }
            None => "Default".to_string(),
        };

        Self {
            index: channel.index,
            name,
        }
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
