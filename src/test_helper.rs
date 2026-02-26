use crate::channel_id::{ChannelId, MessageId, NodeId};
use crate::channel_view_entry::MCContent;
use crate::config::HistoryLength;
use crate::device::Device;
use crate::device::DeviceViewMessage::SubscriptionMessage;
use crate::device::SubscriptionEvent::MyNodeNum;
use crate::meshchat::MCChannel;
use crate::{MeshChat, channel_view_entry};
use std::sync::atomic::{AtomicU32, Ordering};

static MESSAGE_ID_COUNTER: AtomicU32 = AtomicU32::new(1);

pub fn test_app() -> MeshChat {
    let mut meshchat = MeshChat::default();
    let mut device_view = Device::default();
    let _ = device_view.update(SubscriptionMessage(MyNodeNum(NodeId::from(999u64))));
    device_view.add_channel(MCChannel {
        index: 0,
        name: "Test".to_string(),
    });

    meshchat.device = device_view;

    meshchat
}

impl MeshChat {
    pub fn new_message(&mut self, msg: MCContent) {
        let message_id = MESSAGE_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
        let channel_view_entry = channel_view_entry::ChannelViewEntry::new(
            MessageId::from(message_id),
            NodeId::from(1u64),
            msg,
            MeshChat::now(),
        );
        let channel_view = self
            .device
            .channel_views
            .get_mut(&ChannelId::Channel(0.into()))
            .expect("Could not get channel view");
        let _ = channel_view.new_message(channel_view_entry, &HistoryLength::All);
    }
}
