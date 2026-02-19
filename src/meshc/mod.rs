pub mod subscription;

pub const MESHCORE_SERVICE_UUID: Uuid = Uuid::from_u128(0x6e400001_b5a3_f393_e0a9_e50e24dcca9e);

use crate::channel_id::{ChannelId, MessageId, NodeId};
use crate::channel_view_entry::MCMessage;
use crate::device::SubscriptionEvent;
use crate::device::SubscriptionEvent::{MCMessageReceived, NewChannel};
use crate::{MCChannel, MCNodeInfo, MCPosition, MCUser};
use meshcore_rs::commands::Destination;
use meshcore_rs::commands::Destination::Bytes;
use meshcore_rs::events::{
    AdvertisementData, ChannelInfoData, Contact, DiscoverEntry, Neighbour, SelfInfo,
};
use meshcore_rs::{ChannelMessage, ContactMessage};
use uuid::Uuid;

/// Conversions between [SelfIno] and MeshChat [MCUser]
impl From<&SelfInfo> for MCUser {
    fn from(self_info: &SelfInfo) -> Self {
        MCUser {
            #[allow(clippy::unwrap_used)]
            id: node_id_from_public_key(&self_info.public_key).to_string(),
            long_name: self_info.name.clone(),
            short_name: self_info.name.clone(),
            ..Default::default()
        }
    }
}

/// Conversions between [SelfIno] and MeshChat [MCPosition]
impl From<&SelfInfo> for MCPosition {
    fn from(self_info: &SelfInfo) -> Self {
        MCPosition {
            latitude: self_info.adv_lat as f64 / 1_000_000.0,
            longitude: self_info.adv_lon as f64 / 1_000_000.0,
            ..Default::default()
        }
    }
}

impl From<&AdvertisementData> for MCNodeInfo {
    fn from(advert: &AdvertisementData) -> Self {
        MCNodeInfo {
            #[allow(clippy::unwrap_used)]
            node_id: node_id_from_prefix(&advert.prefix),
            user: Some(MCUser {
                #[allow(clippy::unwrap_used)]
                id: node_id_from_prefix(&advert.prefix).to_string(),
                long_name: advert.name.to_string(),
                short_name: advert.name.to_string(), // Use prefix in hex?
                ..Default::default()
            }),
            position: Some(MCPosition {
                latitude: advert.lat as f64 / 1_000_000.0,
                longitude: advert.lon as f64 / 1_000_000.0,
                ..Default::default()
            }),
            is_ignored: false,
        }
    }
}

impl From<Contact> for MCNodeInfo {
    fn from(contact: Contact) -> Self {
        let node_id = node_id_from_prefix(&contact.prefix());
        MCNodeInfo {
            #[allow(clippy::unwrap_used)]
            node_id,
            user: Some(MCUser {
                #[allow(clippy::unwrap_used)]
                id: node_id.to_string(),
                long_name: contact.adv_name.clone(),
                short_name: contact.adv_name.clone(),
                ..Default::default()
            }),
            position: Some(MCPosition {
                latitude: contact.adv_lat as f64 / 1_000_000.0,
                longitude: contact.adv_lon as f64 / 1_000_000.0,
                ..Default::default()
            }),
            is_ignored: false,
        }
    }
}

impl From<ChannelInfoData> for MCChannel {
    fn from(channel: ChannelInfoData) -> Self {
        MCChannel {
            index: channel.channel_idx as i32,
            name: channel.name,
        }
    }
}

impl From<Neighbour> for MCNodeInfo {
    fn from(neighbour: Neighbour) -> Self {
        let node_id = node_id_from_bytes(neighbour.pubkey);
        MCNodeInfo {
            #[allow(clippy::unwrap_used)]
            node_id,
            user: Some(MCUser {
                #[allow(clippy::unwrap_used)]
                id: node_id.to_string(),
                long_name: node_id.to_string(),
                short_name: node_id.to_string(),
                ..Default::default()
            }),
            position: None,
            is_ignored: false,
        }
    }
}

impl From<DiscoverEntry> for MCNodeInfo {
    fn from(discovery: DiscoverEntry) -> Self {
        let node_id = node_id_from_bytes(discovery.pubkey);
        MCNodeInfo {
            #[allow(clippy::unwrap_used)]
            node_id,
            user: Some(MCUser {
                #[allow(clippy::unwrap_used)]
                id: node_id.to_string(),
                long_name: discovery.name.clone(),
                short_name: discovery.name,
                ..Default::default()
            }),
            position: None,
            is_ignored: false,
        }
    }
}

/// Convert from a ChannelMessage to a SubscriptionEvent::MCMessageReceived
impl From<ChannelMessage> for SubscriptionEvent {
    fn from(message: ChannelMessage) -> Self {
        MCMessageReceived(
            ChannelId::Channel(message.channel_idx as i32),
            message.sender_timestamp, // TODO hack for message id in a channel
            0,                        // TODO how to get sender?
            MCMessage::NewTextMessage(message.text),
            message.sender_timestamp,
        )
    }
}

/// Convert from a ContactMessage to a SubscriptionEvent::MCMessageReceived
impl From<ContactMessage> for SubscriptionEvent {
    fn from(message: ContactMessage) -> Self {
        let sender_id = node_id_from_prefix(&message.sender_prefix);
        MCMessageReceived(
            ChannelId::Node(sender_id),
            message.sender_timestamp, // TODO hack for message id
            sender_id,
            MCMessage::NewTextMessage(message.text),
            message.sender_timestamp,
        )
    }
}

impl From<ChannelInfoData> for SubscriptionEvent {
    fn from(channel: ChannelInfoData) -> Self {
        NewChannel(MCChannel {
            index: channel.channel_idx as i32,
            name: channel.name,
        })
    }
}

pub fn node_id_from_prefix(prefix: &[u8; 6]) -> NodeId {
    let mut bytes = [0u8; 8];
    bytes[0..6].copy_from_slice(prefix);
    u64::from_be_bytes(bytes)
}

pub fn node_id_to_destination(node_id: &NodeId) -> Destination {
    Bytes(node_id.to_be_bytes().to_vec())
}

pub fn node_id_from_public_key(public_key: &[u8; 32]) -> NodeId {
    let mut bytes = [0u8; 8];
    bytes.copy_from_slice(&public_key[0..8]);
    u64::from_be_bytes(bytes)
}

pub fn node_id_from_bytes(public_key: Vec<u8>) -> NodeId {
    let mut bytes = [0u8; 8];
    bytes.copy_from_slice(&public_key[0..8]);
    u64::from_be_bytes(bytes)
}

pub fn message_id_from_expected_ack(expected_ack: [u8; 4]) -> MessageId {
    u32::from_be_bytes(expected_ack)
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::channel_id::ChannelId;
    use crate::device::SubscriptionEvent::{MCMessageReceived, NewChannel};
    use crate::meshc::{
        message_id_from_expected_ack, node_id_from_prefix, node_id_from_public_key,
        node_id_to_destination,
    };
    use meshcore_rs::events::{AdvertisementData, ChannelInfoData, Contact, SelfInfo};

    // Tests for node_id_from_prefix

    #[test]
    fn roundtrip_prefix_nodeid() {
        let prefix = [0x01, 0x02, 0x03, 0x04, 0x05, 0x06];
        let node_id = node_id_from_prefix(&prefix);
        let destination = node_id_to_destination(&node_id);
        let prefix2 = destination.prefix().expect("Failed to get prefix");

        assert_eq!(prefix, prefix2, "Failed to roundtrip prefix via node id");
    }

    #[test]
    fn node_id_from_prefix_all_zeros() {
        let prefix = [0x00, 0x00, 0x00, 0x00, 0x00, 0x00];
        let node_id = node_id_from_prefix(&prefix);
        assert_eq!(node_id, 0);
    }

    #[test]
    fn node_id_from_prefix_all_ones() {
        let prefix = [0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF];
        let node_id = node_id_from_prefix(&prefix);
        // Expected: 0xFF_FF_FF_FF_FF_FF_00_00 in big-endian
        assert_eq!(node_id, 0xFFFF_FFFF_FFFF_0000);
    }

    #[test]
    fn node_id_from_prefix_sequential() {
        let prefix = [0x01, 0x02, 0x03, 0x04, 0x05, 0x06];
        let node_id = node_id_from_prefix(&prefix);
        // Expected: 0x01_02_03_04_05_06_00_00 in big-endian
        assert_eq!(node_id, 0x0102_0304_0506_0000);
    }

    // Tests for node_id_from_public_key

    #[test]
    fn node_id_from_public_key_basic() {
        let mut public_key = [0u8; 32];
        public_key[0..8].copy_from_slice(&[0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08]);
        let node_id = node_id_from_public_key(&public_key);
        assert_eq!(node_id, 0x0102_0304_0506_0708);
    }

    #[test]
    fn node_id_from_public_key_all_zeros() {
        let public_key = [0u8; 32];
        let node_id = node_id_from_public_key(&public_key);
        assert_eq!(node_id, 0);
    }

    #[test]
    fn node_id_from_public_key_ignores_trailing_bytes() {
        let mut public_key = [0xAA; 32]; // All bytes set to 0xAA
        public_key[0..8].copy_from_slice(&[0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08]);
        let node_id = node_id_from_public_key(&public_key);
        // Only the first 8 bytes should matter
        assert_eq!(node_id, 0x0102_0304_0506_0708);
    }

    #[test]
    fn node_id_from_public_key_max_value() {
        let mut public_key = [0u8; 32];
        public_key[0..8].copy_from_slice(&[0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF]);
        let node_id = node_id_from_public_key(&public_key);
        assert_eq!(node_id, u64::MAX);
    }

    // Tests for message_id_from_expected_ack

    #[test]
    fn message_id_from_expected_ack_basic() {
        let expected_ack = [0x01, 0x02, 0x03, 0x04];
        let message_id = message_id_from_expected_ack(expected_ack);
        assert_eq!(message_id, 0x01020304);
    }

    #[test]
    fn message_id_from_expected_ack_all_zeros() {
        let expected_ack = [0x00, 0x00, 0x00, 0x00];
        let message_id = message_id_from_expected_ack(expected_ack);
        assert_eq!(message_id, 0);
    }

    #[test]
    fn message_id_from_expected_ack_max_value() {
        let expected_ack = [0xFF, 0xFF, 0xFF, 0xFF];
        let message_id = message_id_from_expected_ack(expected_ack);
        assert_eq!(message_id, u32::MAX);
    }

    // Tests for From<ChannelMessage> for SubscriptionEvent

    #[test]
    fn channel_message_to_subscription_event() {
        let message = ChannelMessage {
            channel_idx: 2,
            path_len: 1,
            txt_type: 0,
            sender_timestamp: 1234567890,
            text: "Hello from channel".to_string(),
            snr: None,
        };

        let event: SubscriptionEvent = message.into();

        let MCMessageReceived(channel_id, _msg_id, _from, msg, timestamp) = event else {
            unreachable!("Expected MCMessageReceived event")
        };

        assert_eq!(channel_id, ChannelId::Channel(2));
        assert_eq!(timestamp, 1234567890);
        assert_eq!(msg.to_string(), "Hello from channel");
    }

    #[test]
    fn channel_message_to_subscription_event_channel_zero() {
        let message = ChannelMessage {
            channel_idx: 0,
            path_len: 2,
            txt_type: 0,
            sender_timestamp: 100,
            text: "Channel 0 message".to_string(),
            snr: None,
        };

        let event: SubscriptionEvent = message.into();

        let MCMessageReceived(channel_id, _, _, _, _) = event else {
            unreachable!("Expected MCMessageReceived event")
        };

        assert_eq!(channel_id, ChannelId::Channel(0));
    }

    // Tests for From<ContactMessage> for SubscriptionEvent

    #[test]
    fn contact_message_to_subscription_event() {
        let message = ContactMessage {
            sender_prefix: [0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF],
            path_len: 0,
            txt_type: 0,
            sender_timestamp: 1234567891,
            text: "Direct message".to_string(),
            snr: Some(10.5),
            signature: None,
        };

        let event: SubscriptionEvent = message.into();

        let MCMessageReceived(channel_id, _msg_id, from, msg, timestamp) = event else {
            unreachable!("Expected MCMessageReceived event")
        };

        // Direct message should use Node channel ID
        let expected_node_id = 0xAABB_CCDD_EEFF_0000_u64;
        assert_eq!(channel_id, ChannelId::Node(expected_node_id));
        assert_eq!(from, expected_node_id);
        assert_eq!(timestamp, 1234567891);
        assert_eq!(msg.to_string(), "Direct message");
    }

    // Tests for From<ChannelInfoData> for SubscriptionEvent

    #[test]
    fn channel_info_to_subscription_event() {
        let channel_info = ChannelInfoData {
            channel_idx: 3,
            name: "LongFast".to_string(),
            secret: [0u8; 16],
        };

        let event: SubscriptionEvent = channel_info.into();

        let NewChannel(channel) = event else {
            unreachable!("Expected NewChannel event")
        };

        assert_eq!(channel.index, 3);
        assert_eq!(channel.name, "LongFast");
    }

    #[test]
    fn channel_info_to_subscription_event_empty_name() {
        let channel_info = ChannelInfoData {
            channel_idx: 0,
            name: "".to_string(),
            secret: [0xFF; 16],
        };

        let event: SubscriptionEvent = channel_info.into();

        let NewChannel(channel) = event else {
            unreachable!("Expected NewChannel event")
        };

        assert_eq!(channel.index, 0);
        assert_eq!(channel.name, "");
    }

    #[test]
    fn channel_info_to_subscription_event_max_index() {
        let channel_info = ChannelInfoData {
            channel_idx: 255,
            name: "HighIndex".to_string(),
            secret: [0u8; 16],
        };

        let event: SubscriptionEvent = channel_info.into();

        let NewChannel(channel) = event else {
            unreachable!("Expected NewChannel event")
        };

        assert_eq!(channel.index, 255);
        assert_eq!(channel.name, "HighIndex");
    }

    // Tests for From<&AdvertisementData> for MCNodeInfo

    #[test]
    fn advertisement_to_node_info() {
        let advert = AdvertisementData {
            prefix: [0x11, 0x22, 0x33, 0x44, 0x55, 0x66],
            name: "TestNode".to_string(),
            lat: 37_774_900, // San Francisco lat in microdegrees
            lon: -122_419_400,
        };

        let node_info: MCNodeInfo = (&advert).into();

        assert_eq!(node_info.node_id, 0x1122_3344_5566_0000);
        assert!(!node_info.is_ignored);

        let user = node_info.user.expect("Expected user");
        assert_eq!(user.long_name, "TestNode");
        assert_eq!(user.short_name, "TestNode");
        assert_eq!(user.id, node_info.node_id.to_string());

        let position = node_info.position.expect("Expected position");
        assert!((position.latitude - 37.7749).abs() < 0.0001);
        assert!((position.longitude - -122.4194).abs() < 0.0001);
    }

    #[test]
    fn advertisement_to_node_info_zero_position() {
        let advert = AdvertisementData {
            prefix: [0x00, 0x00, 0x00, 0x00, 0x00, 0x01],
            name: "ZeroPos".to_string(),
            lat: 0,
            lon: 0,
        };

        let node_info: MCNodeInfo = (&advert).into();

        let position = node_info.position.expect("Expected position");
        assert_eq!(position.latitude, 0.0);
        assert_eq!(position.longitude, 0.0);
    }

    // Tests for From<&Contact> for MCNodeInfo

    #[test]
    fn contact_to_node_info() {
        let contact = Contact {
            public_key: {
                let mut key = [0u8; 32];
                key[0..6].copy_from_slice(&[0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF]);
                key
            },
            contact_type: 0,
            flags: 0,
            path_len: 1,
            out_path: vec![],
            adv_name: "ContactNode".to_string(),
            last_advert: 0,
            adv_lat: 51_507_400, // London lat in microdegrees
            adv_lon: -127_800,
            last_modification_timestamp: 0,
        };

        let node_info: MCNodeInfo = contact.into();

        assert_eq!(node_info.node_id, 0xAABB_CCDD_EEFF_0000);
        assert!(!node_info.is_ignored);

        let user = node_info.user.expect("Expected user");
        assert_eq!(user.long_name, "ContactNode");
        assert_eq!(user.short_name, "ContactNode");

        let position = node_info.position.expect("Expected position");
        assert!((position.latitude - 51.5074).abs() < 0.0001);
        assert!((position.longitude - -0.1278).abs() < 0.0001);
    }

    // Tests for node_id_to_destination

    #[test]
    fn node_id_to_destination_basic() {
        let node_id: NodeId = 0x0102_0304_0506_0000;
        let destination = node_id_to_destination(&node_id);

        let Bytes(bytes) = destination else {
            unreachable!("Expected Bytes destination")
        };

        assert_eq!(bytes.len(), 8);
        assert_eq!(bytes[0..6], [0x01, 0x02, 0x03, 0x04, 0x05, 0x06]);
    }

    #[test]
    fn node_id_to_destination_extracts_prefix() {
        let node_id: NodeId = 0xAABB_CCDD_EEFF_0000;
        let destination = node_id_to_destination(&node_id);
        let prefix = destination.prefix().expect("Should extract prefix");
        assert_eq!(prefix, [0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF]);
    }

    // Helper to create SelfInfo for testing
    fn create_test_self_info(
        name: &str,
        public_key: [u8; 32],
        adv_lat: i32,
        adv_lon: i32,
    ) -> SelfInfo {
        SelfInfo {
            adv_type: 0,
            tx_power: 20,
            max_tx_power: 30,
            public_key,
            adv_lat,
            adv_lon,
            multi_acks: 0,
            adv_loc_policy: 0,
            telemetry_mode_base: 0,
            telemetry_mode_loc: 0,
            telemetry_mode_env: 0,
            manual_add_contacts: false,
            radio_freq: 915_000_000,
            radio_bw: 250_000,
            sf: 12,
            cr: 5,
            name: name.to_string(),
        }
    }

    // Tests for From<&SelfInfo> for MCUser

    #[test]
    fn self_info_to_mcuser() {
        let mut public_key = [0u8; 32];
        public_key[0..8].copy_from_slice(&[0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08]);
        let self_info = create_test_self_info("MyNode", public_key, 0, 0);

        let user: MCUser = (&self_info).into();

        assert_eq!(user.long_name, "MyNode");
        assert_eq!(user.short_name, "MyNode");
        assert_eq!(user.id, 0x0102_0304_0506_0708_u64.to_string());
    }

    #[test]
    fn self_info_to_mcuser_empty_name() {
        let public_key = [0u8; 32];
        let self_info = create_test_self_info("", public_key, 0, 0);

        let user: MCUser = (&self_info).into();

        assert_eq!(user.long_name, "");
        assert_eq!(user.short_name, "");
    }

    #[test]
    fn self_info_to_mcuser_unicode_name() {
        let public_key = [0xFF; 32];
        let self_info = create_test_self_info("日本語ノード", public_key, 0, 0);

        let user: MCUser = (&self_info).into();

        assert_eq!(user.long_name, "日本語ノード");
        assert_eq!(user.short_name, "日本語ノード");
    }

    // Tests for From<&SelfInfo> for MCPosition

    #[test]
    fn self_info_to_mcposition() {
        let public_key = [0u8; 32];
        // San Francisco coordinates in microdegrees
        let self_info = create_test_self_info("TestNode", public_key, 37_774_900, -122_419_400);

        let position: MCPosition = (&self_info).into();

        assert!((position.latitude - 37.7749).abs() < 0.0001);
        assert!((position.longitude - -122.4194).abs() < 0.0001);
    }

    #[test]
    fn self_info_to_mcposition_zero() {
        let public_key = [0u8; 32];
        let self_info = create_test_self_info("ZeroNode", public_key, 0, 0);

        let position: MCPosition = (&self_info).into();

        assert_eq!(position.latitude, 0.0);
        assert_eq!(position.longitude, 0.0);
    }

    #[test]
    fn self_info_to_mcposition_negative() {
        let public_key = [0u8; 32];
        // Sydney coordinates in microdegrees
        let self_info = create_test_self_info("SydneyNode", public_key, -33_868_800, 151_209_300);

        let position: MCPosition = (&self_info).into();

        assert!((position.latitude - -33.8688).abs() < 0.0001);
        assert!((position.longitude - 151.2093).abs() < 0.0001);
    }

    #[test]
    fn self_info_to_mcposition_extremes() {
        let public_key = [0u8; 32];
        // Near max latitude/longitude in microdegrees
        let self_info = create_test_self_info("ExtremeNode", public_key, 89_999_999, 179_999_999);

        let position: MCPosition = (&self_info).into();

        assert!((position.latitude - 89.999999).abs() < 0.000001);
        assert!((position.longitude - 179.999999).abs() < 0.000001);
    }
}
