use crate::SubscriptionEvent::MyNodeNum;
use crate::channel_id::ChannelId;
use crate::channel_view_entry::MCMessage;
use crate::config::HistoryLength;
use crate::device::Device;
use crate::device::DeviceViewMessage::SubscriptionMessage;
use crate::{MCChannel, MeshChat, channel_view_entry};
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

static MESSAGE_ID_COUNTER: AtomicU32 = AtomicU32::new(1);

pub fn test_app() -> MeshChat {
    let mut meshchat = MeshChat::default();
    let mut device_view = Device::default();
    let _ = device_view.update(SubscriptionMessage(MyNodeNum(999)));
    device_view.add_channel(MCChannel {
        index: 0,
        name: "Test".to_string(),
    });

    meshchat.device = device_view;

    meshchat
}

impl MeshChat {
    pub fn new_message(&mut self, msg: MCMessage) {
        let message_id = MESSAGE_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
        let channel_view_entry = channel_view_entry::ChannelViewEntry::new(
            message_id,
            1,
            msg,
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("Could not get time")
                .as_secs() as u32,
        );
        let channel_view = self
            .device
            .channel_views
            .get_mut(&ChannelId::Channel(0))
            .expect("Could not get channel view");
        let _ = channel_view.new_message(channel_view_entry, &HistoryLength::All);
    }
}
