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
        let lat = position.latitude_i.unwrap_or(0);
        let latitude = 0.0000001 * lat as f64;
        let long = position.longitude_i.unwrap_or(0);
        let longitude = 0.0000001 * long as f64;

        MCPosition {
            latitude,
            longitude,
            timestamp: position.timestamp,
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

#[allow(clippy::from_over_into)]
impl Into<Position> for MCPosition {
    fn into(self) -> Position {
        let lat = self.latitude as i32 * 1000000;
        let long = self.longitude as i32 * 1000000;
        Position {
            latitude_i: Some(lat),
            longitude_i: Some(long),
            timestamp: self.timestamp,
            altitude: self.altitude,
            time: self.time,
            location_source: self.location_source,
            altitude_source: self.altitude_source,
            timestamp_millis_adjust: self.timestamp_millis_adjust,
            altitude_hae: self.altitude_hae,
            altitude_geoidal_separation: self.altitude_geoidal_separation,
            pdop: self.pdop,
            hdop: self.hdop,
            vdop: self.vdop,
            gps_accuracy: self.gps_accuracy,
            ground_speed: self.ground_speed,
            ground_track: self.ground_track,
            fix_quality: self.fix_quality,
            fix_type: self.fix_type,
            sats_in_view: self.sats_in_view,
            sensor_id: self.sensor_id,
            next_update: self.next_update,
            seq_number: self.seq_number,
            precision_bits: self.precision_bits,
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
