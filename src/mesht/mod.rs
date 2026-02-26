use crate::conversation_id::ConversationId::{Channel, Node};
use crate::conversation_id::{ChannelIndex, ConversationId};
use crate::meshchat::{MCChannel, MCNodeInfo, MCPosition, MCUser};
use meshtastic::packet::PacketDestination;
use meshtastic::protobufs::{NodeInfo, Position, User};
use meshtastic::types::{MeshChannel, NodeId};
use uuid::Uuid;

pub mod subscription;

pub const MESHTASTIC_SERVICE_UUID: Uuid = Uuid::from_u128(0x6ba1b218_15a8_461f_9fa8_5dcae273eafd);

/// Conversions between [User] and MeshChat [MCUser]
impl From<&User> for MCUser {
    fn from(user: &User) -> Self {
        MCUser {
            id: user.id.clone(),
            long_name: user.long_name.clone(),
            short_name: user.short_name.clone(),
            hw_model_str: user.hw_model().as_str_name().to_string(),
            hw_model: user.hw_model,
            is_licensed: user.is_licensed,
            role_str: user.role().as_str_name().to_string(),
            role: user.role,
            public_key: user.public_key.clone(),
            is_unmessagable: user.is_unmessagable.unwrap_or(false),
        }
    }
}

/// Conversions between [MCUser] and MeshChat [User]
impl From<MCUser> for User {
    fn from(user: MCUser) -> Self {
        User {
            id: user.id.clone(),
            long_name: user.long_name.clone(),
            short_name: user.short_name.clone(),
            hw_model: user.hw_model,
            is_licensed: user.is_licensed,
            role: user.role,
            public_key: user.public_key.clone(),
            is_unmessagable: Some(user.is_unmessagable),
            ..Default::default()
        }
    }
}

/// Conversions between [NodeInfo] and MeshChat [MCNodeInfo]
impl From<&NodeInfo> for MCNodeInfo {
    fn from(node_info: &NodeInfo) -> Self {
        MCNodeInfo {
            node_id: node_info.num.into(),
            user: node_info.user.as_ref().map(|u| u.into()),
            position: node_info.position.as_ref().map(|p| p.into()),
            is_ignored: node_info.is_ignored,
        }
    }
}

/// Conversions between [Position] and MeshChat [MCPosition]
impl From<&Position> for MCPosition {
    fn from(position: &Position) -> Self {
        let lat = position.latitude_i.unwrap_or(0);
        let latitude = 0.0000001 * lat as f64;
        let long = position.longitude_i.unwrap_or(0);
        let longitude = 0.0000001 * long as f64;

        MCPosition {
            latitude,
            longitude,
            timestamp: position.timestamp.into(),
            altitude: position.altitude,
            time: position.time,
            location_source: position.location_source,
            altitude_source: position.altitude_source,
            timestamp_millis_adjust: position.timestamp_millis_adjust,
            altitude_hae: position.altitude_hae,
            altitude_geoidal_separation: position.altitude_geoidal_separation,
            pdop: position.pdop,
            hdop: position.hdop,
            vdop: position.vdop,
            gps_accuracy: position.gps_accuracy,
            ground_speed: position.ground_speed,
            ground_track: position.ground_track,
            fix_quality: position.fix_quality,
            fix_type: position.fix_type,
            sats_in_view: position.sats_in_view,
            sensor_id: position.sensor_id,
            next_update: position.next_update,
            seq_number: position.seq_number,
            precision_bits: position.precision_bits,
        }
    }
}

impl From<MCPosition> for Position {
    fn from(value: MCPosition) -> Position {
        let lat = (value.latitude * 10_000_000.0) as i32;
        let long = (value.longitude * 10_000_000.0) as i32;
        Position {
            latitude_i: Some(lat),
            longitude_i: Some(long),
            timestamp: value.timestamp.into(),
            altitude: value.altitude,
            time: value.time,
            location_source: value.location_source,
            altitude_source: value.altitude_source,
            timestamp_millis_adjust: value.timestamp_millis_adjust,
            altitude_hae: value.altitude_hae,
            altitude_geoidal_separation: value.altitude_geoidal_separation,
            pdop: value.pdop,
            hdop: value.hdop,
            vdop: value.vdop,
            gps_accuracy: value.gps_accuracy,
            ground_speed: value.ground_speed,
            ground_track: value.ground_track,
            fix_quality: value.fix_quality,
            fix_type: value.fix_type,
            sats_in_view: value.sats_in_view,
            sensor_id: value.sensor_id,
            next_update: value.next_update,
            seq_number: value.seq_number,
            precision_bits: value.precision_bits,
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

impl From<ChannelIndex> for MeshChannel {
    fn from(value: ChannelIndex) -> Self {
        MeshChannel::from(<ChannelIndex as Into<u32>>::into(value))
    }
}

impl ConversationId {
    pub fn to_destination(self) -> (PacketDestination, MeshChannel) {
        match self {
            Channel(channel_number) => (
                PacketDestination::Broadcast,
                MeshChannel::from(channel_number),
            ),
            Node(node_id) => (
                PacketDestination::Node(NodeId::new(node_id.into())),
                MeshChannel::default(),
            ),
        }
    }
}

#[cfg(test)]
#[allow(clippy::panic)]
mod test {
    use super::*;
    use crate::conversation_id;
    use crate::meshchat::{MCChannel, MCNodeInfo, MCPosition, MCUser};
    use crate::timestamp::TimeStamp;
    use meshtastic::packet::PacketDestination;
    use meshtastic::protobufs::{ChannelSettings, Position, User};
    use meshtastic::types::MeshChannel;

    #[test]
    fn test_to_channel() {
        let conversation_id = Channel(0.into());
        let (destination, channel) = conversation_id.to_destination();
        assert!(
            matches!(destination, PacketDestination::Broadcast),
            "Channel destination should be Broadcast, got {:?}",
            destination
        );
        assert_eq!(channel, MeshChannel::from(0));
    }

    #[test]
    fn test_to_node_destination() {
        let conversation_id = Node(conversation_id::NodeId::from(12345u32));
        let (destination, channel) = conversation_id.to_destination();
        assert!(
            matches!(destination, PacketDestination::Node(_)),
            "Node destination should be Node variant, got {:?}",
            destination
        );
        assert_eq!(channel, MeshChannel::default());
    }

    #[test]
    fn test_user_conversion_from() {
        let user = User {
            id: "test_id".to_string(),
            long_name: "Test User".to_string(),
            short_name: "TEST".to_string(),
            hw_model: 0,
            is_licensed: true,
            role: 0,
            public_key: vec![1, 2, 3],
            is_unmessagable: Some(false),
            ..Default::default()
        };

        let mc_user: MCUser = (&user).into();

        assert_eq!(mc_user.id, "test_id");
        assert_eq!(mc_user.long_name, "Test User");
        assert_eq!(mc_user.short_name, "TEST");
        assert!(mc_user.is_licensed);
        assert_eq!(mc_user.public_key, vec![1, 2, 3]);
        assert!(!mc_user.is_unmessagable);
    }

    #[test]
    fn test_user_conversion_to() {
        let mc_user = MCUser {
            id: "test_id".to_string(),
            long_name: "Test User".to_string(),
            short_name: "TEST".to_string(),
            hw_model_str: "TBEAM".to_string(),
            hw_model: 4,
            is_licensed: false,
            role_str: "CLIENT".to_string(),
            role: 0,
            public_key: vec![4, 5, 6],
            is_unmessagable: true,
        };

        let user: User = mc_user.into();

        assert_eq!(user.id, "test_id");
        assert_eq!(user.long_name, "Test User");
        assert_eq!(user.short_name, "TEST");
        assert!(!user.is_licensed);
        assert_eq!(user.public_key, vec![4, 5, 6]);
        assert_eq!(user.is_unmessagable, Some(true));
    }

    #[test]
    fn test_user_unmessagable_none_defaults_to_false() {
        let user = User {
            id: "test".to_string(),
            is_unmessagable: None,
            ..Default::default()
        };

        let mc_user: MCUser = (&user).into();
        assert!(!mc_user.is_unmessagable);
    }

    #[test]
    fn test_position_conversion_from() {
        let position = Position {
            latitude_i: Some(500000000), // 50.0 degrees
            longitude_i: Some(10000000), // 1.0 degree
            altitude: Some(100),
            timestamp: 12345,
            time: 67890,
            ..Default::default()
        };

        let mc_position: MCPosition = (&position).into();

        assert!((mc_position.latitude - 50.0).abs() < 0.0001);
        assert!((mc_position.longitude - 1.0).abs() < 0.0001);
        assert_eq!(mc_position.altitude, Some(100));
        assert_eq!(mc_position.timestamp, TimeStamp::from(12345u64));
        assert_eq!(mc_position.time, 67890);
    }

    #[test]
    fn test_position_conversion_to() {
        let mc_position = MCPosition {
            latitude: 50.0,
            longitude: 1.0,
            altitude: Some(200),
            time: 0,
            location_source: 0,
            altitude_source: 0,
            timestamp: TimeStamp::from(0u64),
            timestamp_millis_adjust: 0,
            altitude_hae: None,
            altitude_geoidal_separation: None,
            pdop: 0,
            hdop: 0,
            vdop: 0,
            gps_accuracy: 0,
            ground_speed: None,
            ground_track: None,
            fix_quality: 0,
            fix_type: 0,
            sats_in_view: 0,
            sensor_id: 0,
            next_update: 0,
            seq_number: 0,
            precision_bits: 0,
        };

        let position: Position = mc_position.into();

        assert_eq!(position.latitude_i, Some(500000000));
        assert_eq!(position.longitude_i, Some(10000000));
        assert_eq!(position.altitude, Some(200));
    }

    #[test]
    fn test_position_none_coordinates() {
        let position = Position {
            latitude_i: None,
            longitude_i: None,
            ..Default::default()
        };

        let mc_position: MCPosition = (&position).into();

        assert_eq!(mc_position.latitude, 0.0);
        assert_eq!(mc_position.longitude, 0.0);
    }

    #[test]
    fn test_channel_conversion_with_name() {
        let channel = meshtastic::protobufs::Channel {
            index: 1,
            settings: Some(ChannelSettings {
                name: "MyChannel".to_string(),
                ..Default::default()
            }),
            ..Default::default()
        };

        let mc_channel: MCChannel = (&channel).into();

        assert_eq!(mc_channel.index, 1);
        assert_eq!(mc_channel.name, "MyChannel");
    }

    #[test]
    fn test_channel_conversion_empty_name() {
        let channel = meshtastic::protobufs::Channel {
            index: 0,
            settings: Some(ChannelSettings {
                name: "".to_string(),
                ..Default::default()
            }),
            ..Default::default()
        };

        let mc_channel: MCChannel = (&channel).into();

        assert_eq!(mc_channel.index, 0);
        assert_eq!(mc_channel.name, "Default");
    }

    #[test]
    fn test_channel_conversion_no_settings() {
        let channel = meshtastic::protobufs::Channel {
            index: 2,
            settings: None,
            ..Default::default()
        };

        let mc_channel: MCChannel = (&channel).into();

        assert_eq!(mc_channel.index, 2);
        assert_eq!(mc_channel.name, "Default");
    }

    #[test]
    fn test_node_info_conversion() {
        let user = User {
            id: "test".to_string(),
            long_name: "Test Node".to_string(),
            short_name: "TN".to_string(),
            ..Default::default()
        };

        let node_info = NodeInfo {
            num: 12345,
            user: Some(user),
            position: None,
            channel: 0,
            is_ignored: false,
            ..Default::default()
        };

        let mc_node_info: MCNodeInfo = (&node_info).into();

        assert_eq!(
            mc_node_info.node_id,
            conversation_id::NodeId::from(12345_u32)
        );
        assert!(mc_node_info.user.is_some());
        assert_eq!(
            mc_node_info
                .user
                .as_ref()
                .expect("Could not get user")
                .long_name,
            "Test Node"
        );
        assert!(mc_node_info.position.is_none());
        assert!(!mc_node_info.is_ignored);
    }

    #[test]
    fn test_node_info_with_position() {
        let position = Position {
            latitude_i: Some(510000000),
            longitude_i: Some(-1000000),
            ..Default::default()
        };

        let node_info = NodeInfo {
            num: 99999,
            user: None,
            position: Some(position),
            channel: 1,
            is_ignored: true,
            ..Default::default()
        };

        let mc_node_info: MCNodeInfo = (&node_info).into();

        assert_eq!(
            mc_node_info.node_id,
            conversation_id::NodeId::from(99999_u32)
        );
        assert!(mc_node_info.user.is_none());
        assert!(mc_node_info.position.is_some());
        assert!(mc_node_info.is_ignored);
    }
}
