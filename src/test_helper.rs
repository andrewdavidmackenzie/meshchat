use crate::channel_id::ChannelId;
use crate::channel_view_entry::MCMessage;
use crate::config::HistoryLength;
use crate::device_view::DeviceView;
use crate::device_view::DeviceViewMessage::SubscriptionMessage;
use crate::meshtastic::device_subscription::SubscriptionEvent::MyNodeNum;
use crate::{MCChannel, MeshChat, channel_view_entry};

pub fn test_app() -> MeshChat {
    let mut meshchat = MeshChat::default();
    let mut device_view = DeviceView::default();
    let _ = device_view.update(SubscriptionMessage(MyNodeNum(999)));
    device_view.add_channel(MCChannel {
        index: 0,
        name: "Test".to_string(),
    });

    meshchat.device_view = device_view;

    meshchat
}

impl MeshChat {
    pub fn new_message(&mut self, msg: MCMessage) {
        let channel_view_entry = channel_view_entry::ChannelViewEntry::new(0, 1, msg);
        let channel_view = self
            .device_view
            .channel_views
            .get_mut(&ChannelId::Channel(0))
            .unwrap();
        let _ = channel_view.new_message(channel_view_entry, &HistoryLength::All);
    }
}
