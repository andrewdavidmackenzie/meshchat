use crate::channel_id::ChannelId;
use crate::channel_view_entry::Payload;
use crate::config::HistoryLength;
use crate::device_subscription::SubscriptionEvent::DevicePacket;
use crate::device_view::DeviceView;
use crate::device_view::DeviceViewMessage::SubscriptionMessage;
use crate::{MCChannel, MeshChat, channel_view_entry};
use meshtastic::protobufs::from_radio::PayloadVariant;
use meshtastic::protobufs::{FromRadio, MyNodeInfo};

pub fn test_app() -> MeshChat {
    let mut meshchat = MeshChat::default();
    let mut device_view = DeviceView::default();
    let radio_packet = FromRadio {
        payload_variant: Some(PayloadVariant::MyInfo(MyNodeInfo {
            my_node_num: 999,
            reboot_count: 0,
            min_app_version: 0,
            device_id: vec![],
            pio_env: "".to_string(),
            firmware_edition: 0,
            nodedb_count: 0,
        })),
        ..Default::default()
    };

    let _ = device_view.update(SubscriptionMessage(DevicePacket(Box::new(radio_packet))));

    let channel = MCChannel {
        index: 0,
        name: "Test".to_string(),
    };

    device_view.add_channel(channel);

    meshchat.device_view = device_view;

    meshchat
}

impl MeshChat {
    pub fn new_message(&mut self, msg: Payload) {
        let channel_view_entry = channel_view_entry::ChannelViewEntry::new(msg, 0, 1);
        let channel_view = self
            .device_view
            .channel_views
            .get_mut(&ChannelId::Channel(0))
            .unwrap();
        let _ = channel_view.new_message(channel_view_entry, &HistoryLength::All);
    }
}
