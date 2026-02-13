pub mod subscription;

pub const MESHCORE_SERVICE_UUID: Uuid = Uuid::from_u128(0x6e400001_b5a3_f393_e0a9_e50e24dcca9e);

use crate::{MCNodeInfo, MCPosition, MCUser};
use meshcore_rs::events::{AdvertisementData, Contact, SelfInfo};
use uuid::Uuid;

/// Conversions between [SelfIno] and MeshChat [MCUser]
impl From<&SelfInfo> for MCUser {
    fn from(self_info: &SelfInfo) -> Self {
        MCUser {
            #[allow(clippy::unwrap_used)]
            id: u32::from_be_bytes(self_info.public_key[0..4].try_into().unwrap()).to_string(), // TODO
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
            num: u32::from_be_bytes(advert.prefix[0..4].try_into().unwrap()), // TODO
            user: Some(MCUser {
                #[allow(clippy::unwrap_used)]
                id: u32::from_be_bytes(advert.prefix[0..4].try_into().unwrap()).to_string(), // TODO
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

impl From<&Contact> for MCNodeInfo {
    fn from(contact: &Contact) -> Self {
        MCNodeInfo {
            #[allow(clippy::unwrap_used)]
            num: u32::from_be_bytes(contact.prefix()[0..4].try_into().unwrap()), // TODO
            user: Some(MCUser {
                #[allow(clippy::unwrap_used)]
                id: u32::from_be_bytes(contact.prefix()[0..4].try_into().unwrap()).to_string(), // TODO
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

/*
/// Convert a [ReceivedMessage] from MeshCore radio into a [SubscriptionEvent] for the GUI
impl From<ReceivedMessage> for SubscriptionEvent {
    fn from(message: ReceivedMessage) -> Self {
        let channel_id = if let Some(channel_index) = message.channel {
            ChannelId::Channel(channel_index as i32)
        } else {
            ChannelId::Node(0) // TODO figure this out
        };
        MCMessageReceived(
            channel_id,
            mesh_packet.id,
            mesh_packet.from,
            NewTextMessage(message.text),
            message.sender_timestamp,
        )
    }
}


 */
