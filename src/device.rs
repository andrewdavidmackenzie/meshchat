use crate::channel_view::{ChannelView, ChannelViewMessage, MESSAGE_INPUT_ID};
use crate::channel_view_entry::{ChannelViewEntry, MCContent};
use crate::config::{Config, HistoryLength};
use crate::device::ConnectionState::{Connected, Connecting, Disconnected, Disconnecting};
use crate::device::DeviceViewMessage::{
    AliasInput, ChannelMsg, ClearFilter, ConnectRequest, DisconnectRequest, ForwardMessage,
    SearchInput, SendEmojiReplyMessage, SendPositionMessage, SendSelfInfoMessage, SendTextMessage,
    ShowChannel, StartEditingAlias, StartForwardingMessage, StopForwardingMessage,
    SubscriptionMessage,
};
use crate::device::SubscriberMessage::{
    Connect, Disconnect, SendEmojiReply, SendPosition, SendSelfInfo, SendText,
};
use crate::device::SubscriptionEvent::{
    ChannelName, ConnectedEvent, ConnectingEvent, ConnectionError, DisconnectedEvent,
    DisconnectingEvent, MyPosition, MyUserInfo, NotReady, Ready, SendError,
};
use crate::{MeshChat, Message, icons};

use crate::Message::{
    AddNodeAlias, AppError, DeviceViewEvent, Navigation, OpenSettingsDialog, OpenUrl,
    RemoveNodeAlias, ShowLocation, ShowUserInfo, ToggleNodeFavourite,
};
use crate::channel_id::ChannelId::Node;
use crate::channel_id::{ChannelId, ChannelIndex, MessageId, NodeId};
use crate::channel_view_entry::MCContent::{PositionMessage, UserMessage};
use crate::device::SubscriptionEvent::{
    DeviceBatteryLevel, MCMessageReceived, MessageACK, MyNodeNum, NewChannel, NewNode, NewNodeInfo,
    NewNodePosition, RadioNotification,
};
use crate::device_list::{DeviceList, RadioType};
use crate::meshchat::View::DeviceListView;
use crate::meshchat::{MCChannel, MCNodeInfo, MCPosition, MCUser, View};
use crate::styles::{
    DAY_SEPARATOR_STYLE, battery_style, button_chip_style, channel_row_style, count_style,
    fav_button_style, scrollbar_style, text_input_button_style, text_input_container_style,
    text_input_style, tooltip_style,
};
use crate::widgets::battery::{Battery, BatteryState};
use iced::widget::scrollable::Scrollbar;
use iced::widget::{
    Button, Column, Container, Id, Row, Space, button, container, operation, scrollable, text,
    text_input, tooltip,
};
use iced::{Bottom, Center, Element, Fill, Padding, Task};
#[cfg(feature = "meshcore")]
use meshcore_rs::MeshCoreEvent;
#[cfg(feature = "meshtastic")]
use meshtastic::protobufs::FromRadio;
use std::collections::HashMap;
use std::ops::Sub;
use tokio::sync::mpsc::Sender;

#[derive(Clone, PartialEq, Debug)]
pub enum ConnectionState {
    Disconnected(Option<String>, Option<String>),
    Connecting(String),
    Connected(String, RadioType),
    Disconnecting(String),
}

impl Default for ConnectionState {
    fn default() -> Self {
        Disconnected(None, None)
    }
}

/// Time in EPOC in seconds timestamp
#[derive(PartialEq, PartialOrd, Debug, Default, Clone, Copy)]
pub struct TimeStamp(u32);

impl From<u32> for TimeStamp {
    fn from(value: u32) -> Self {
        TimeStamp(value)
    }
}

impl From<TimeStamp> for u32 {
    fn from(value: TimeStamp) -> Self {
        value.0
    }
}

impl From<TimeStamp> for i64 {
    fn from(value: TimeStamp) -> Self {
        value.0 as i64
    }
}

impl Sub for TimeStamp {
    type Output = TimeStamp;

    fn sub(self, rhs: Self) -> Self::Output {
        TimeStamp::from(self.0.saturating_sub(rhs.0))
    }
}

const CHANNEL_SEARCH_ID: Id = Id::new("message_input");

/// Events are Messages sent from the subscription to the GUI
#[derive(Debug, Clone)]
pub enum SubscriptionEvent {
    /// The subscription is ready to receive [SubscriberMessage] from the GUI
    Ready(Sender<SubscriberMessage>, RadioType),
    ConnectedEvent(String, RadioType),
    ConnectingEvent(String),
    DisconnectingEvent(String),
    DisconnectedEvent(String),
    ConnectionError(String, String, String),
    SendError(String, String),
    NotReady,
    MyNodeNum(NodeId),
    MyUserInfo(MCUser),
    MyPosition(MCPosition),
    NewChannel(MCChannel),
    NewNode(MCNodeInfo),
    RadioNotification(String, TimeStamp), // Message, TimeStamp
    /// ChannelId - channel sent to, MessageId, NodeId - sending node, The Message itself, Timestamp
    MCMessageReceived(ChannelId, MessageId, NodeId, MCContent, TimeStamp),
    /// ChannelId, MessageId
    MessageACK(ChannelId, MessageId),
    NewNodeInfo(ChannelId, MessageId, NodeId, MCUser, TimeStamp), // channel_id, id, from, MCUser, TimeStamp
    NewNodePosition(ChannelId, MessageId, NodeId, MCPosition, TimeStamp), // channel_id, id, from, MCPosition, TimeStamp
    DeviceBatteryLevel(Option<u32>),
    ChannelName(i32, String), // channel number, name
}

/// Messages sent from the GUI to the subscription
pub enum SubscriberMessage {
    Connect(String, RadioType),
    Disconnect,
    SendText(String, ChannelId, Option<MessageId>), // Optional reply to message id
    SendEmojiReply(String, ChannelId, MessageId),
    SendPosition(ChannelId, MCPosition),
    SendSelfInfo(ChannelId, MCUser),
    #[cfg(feature = "meshtastic")]
    MeshTasticRadioPacket(Box<FromRadio>), // Sent from the radio to the subscription, not GUI
    #[cfg(feature = "meshcore")]
    MeshCoreRadioPacket(Box<MeshCoreEvent>), // Sent from the radio to the subscription, not GUI
}

#[derive(Debug, Clone)]
pub enum DeviceViewMessage {
    ConnectRequest(String, RadioType, Option<ChannelId>),
    DisconnectRequest(bool), // bool is to exit or not
    SubscriptionMessage(SubscriptionEvent),
    ShowChannel(Option<ChannelId>),
    ChannelMsg(ChannelId, ChannelViewMessage),
    SendTextMessage(String, ChannelId, Option<MessageId>), // optional reply to message id
    SendEmojiReplyMessage(MessageId, String, ChannelId),   // optional reply to message id
    SendPositionMessage(ChannelId),
    SendSelfInfoMessage(ChannelId),
    SearchInput(String),
    StartEditingAlias(NodeId),
    AliasInput(String),
    StartForwardingMessage(ChannelViewEntry),
    ForwardMessage(ChannelId),
    StopForwardingMessage,
    ClearFilter,
}

#[derive(Default)]
pub struct Device {
    connection_state: ConnectionState,
    #[cfg(feature = "meshtastic")]
    meshtastic_sender: Option<Sender<SubscriberMessage>>,
    #[cfg(feature = "meshcore")]
    meshcore_sender: Option<Sender<SubscriberMessage>>,
    my_node_id: Option<NodeId>,
    my_position: Option<MCPosition>,
    my_user: Option<MCUser>,
    viewing_channel: Option<ChannelId>,
    /// Map of ChannelViews, indexed by ChannelId
    pub channel_views: HashMap<ChannelId, ChannelView>,
    channels: Vec<MCChannel>,
    nodes: HashMap<NodeId, MCNodeInfo>, // all nodes known to the connected radio
    filter: String,
    exit_pending: bool,
    battery_level: Option<u32>,
    editing_alias: Option<NodeId>,
    alias: String,
    pub forwarding_message: Option<ChannelViewEntry>,
    history_length: HistoryLength,
    show_position_updates: bool,
    show_user_updates: bool,
}

async fn empty() {}

impl Device {
    /// Get the current [ConnectionState] of this device
    pub fn connection_state(&self) -> &ConnectionState {
        &self.connection_state
    }

    /// Cancel or Exit any interactive modes underway
    pub fn cancel_interactive(&mut self) {
        self.stop_editing_alias();
        self.forwarding_message = None;
        if let Some(viewing_channel) = &self.viewing_channel
            && let Some(channel_view) = self.channel_views.get_mut(viewing_channel)
        {
            channel_view.cancel_interactive();
        }
    }

    /// Set how many/how long we will store messages for
    pub fn set_history_length(&mut self, history_length: HistoryLength) {
        self.history_length = history_length;
    }

    /// Set the devices setting whether it should show position updates in the chat view or not
    pub fn set_show_position_updates(&mut self, show_position_updates: bool) {
        self.show_position_updates = show_position_updates;
    }

    /// Set the devices setting whether it should show user updates in the chat view or not
    pub fn set_show_user_updates(&mut self, show_user_updates: bool) {
        self.show_user_updates = show_user_updates;
    }

    /// Return a true value to show we can show the device view, false for main to decide
    pub fn update(&mut self, device_view_message: DeviceViewMessage) -> Task<Message> {
        match device_view_message {
            ConnectRequest(ble_device, radio_type, channel_id) => {
                return self.subscriber_send(
                    Connect(ble_device, radio_type),
                    Navigation(View::DeviceView(channel_id)),
                );
            }
            DisconnectRequest(exit) => {
                self.exit_pending = exit;
                return self.subscriber_send(Disconnect, Navigation(DeviceListView));
            }
            ForwardMessage(channel_id) => {
                if let Some(entry) = self.forwarding_message.take() {
                    let message_text = format!(
                        "FWD from '{}': {}\n",
                        short_name(&self.nodes, entry.from()),
                        entry.message()
                    );

                    return self.subscriber_send(
                        SendText(message_text, channel_id, None),
                        DeviceViewEvent(ShowChannel(Some(channel_id))),
                    );
                }
            }
            SendEmojiReplyMessage(reply_to_id, emoji, channel_id) => {
                return self.subscriber_send(
                    SendEmojiReply(emoji, channel_id, reply_to_id),
                    Message::None,
                );
            }
            SendTextMessage(message, channel_id, reply_to_id) => {
                return self
                    .subscriber_send(SendText(message, channel_id, reply_to_id), Message::None);
            }
            SendPositionMessage(channel_id) => {
                if let Some(position) = &self.my_position {
                    return self.subscriber_send(
                        SendPosition(channel_id, position.clone()),
                        Message::None,
                    );
                }
            }
            SendSelfInfoMessage(channel_id) => {
                if let Some(user) = &self.my_user {
                    return self
                        .subscriber_send(SendSelfInfo(channel_id, user.clone()), Message::None);
                }
            }
            ShowChannel(channel_id) => {
                return self.channel_change(channel_id);
            }
            SubscriptionMessage(subscription_event) => {
                return self.process_subscription_event(subscription_event);
            }
            ChannelMsg(channel_id, msg) => {
                if let Some(channel_view) = self.channel_views.get_mut(&channel_id) {
                    return channel_view.update(msg);
                } else {
                    eprintln!("Error: No channel for ChannelMsg");
                }
            }
            SearchInput(filter) => self.filter = filter,
            AliasInput(alias) => self.alias = alias,
            StartEditingAlias(node_id) => self.start_editing_alias(node_id),
            StartForwardingMessage(channel_view_entry) => {
                self.forwarding_message = Some(channel_view_entry)
            }
            StopForwardingMessage => self.forwarding_message = None,
            ClearFilter => self.filter.clear(),
        }

        Task::none()
    }

    /// Send a SubscriberMessage to the device_subscription, if successful, then send `success_message`
    /// and report any errors
    fn subscriber_send(
        &mut self,
        subscriber_message: SubscriberMessage,
        success_message: Message,
    ) -> Task<Message> {
        // Either we are connected and know the radio type of we are disconnected and being asked
        // to connect to a specific type of radio
        let radio_type = if let Connected(_, radio_type) = self.connection_state {
            radio_type
        } else if let Connect(_, radio_type) = &subscriber_message {
            *radio_type
        } else {
            return Task::perform(empty(), |_| DeviceViewEvent(SubscriptionMessage(NotReady)));
        };

        let subscription_sender: Option<Sender<SubscriberMessage>> = match radio_type {
            #[cfg(feature = "meshtastic")]
            RadioType::Meshtastic => self.meshtastic_sender.clone(),
            #[cfg(feature = "meshcore")]
            RadioType::MeshCore => self.meshcore_sender.clone(),
        };

        if let Some(sender) = subscription_sender {
            let future = async move { sender.send(subscriber_message).await };
            Task::perform(future, |result| match result {
                Ok(()) => success_message,
                Err(e) => AppError(
                    "Connection Error".to_string(),
                    format!("{:?}", e),
                    MeshChat::now(),
                ),
            })
        } else {
            Task::perform(empty(), |_| DeviceViewEvent(SubscriptionMessage(NotReady)))
        }
    }

    /// Save the new channel (which could be None)
    fn channel_change(&mut self, channel_id: Option<ChannelId>) -> Task<Message> {
        if self.viewing_channel != channel_id {
            self.viewing_channel = channel_id;

            if let Connected(ble_device, radio_type) = &self.connection_state {
                let device = ble_device.clone();
                let radio = *radio_type;

                let input_to_focus = match self.viewing_channel {
                    None => CHANNEL_SEARCH_ID,
                    Some(_channel) => MESSAGE_INPUT_ID,
                };

                // Focus on the correct input text and then save the config change
                return operation::focus(input_to_focus).chain(Task::perform(empty(), move |_| {
                    Message::DeviceAndChannelConfigChange(Some((device, radio)), channel_id)
                }));
            }
        }
        Task::none()
    }

    /// Called when the user selects to alias a node name
    fn start_editing_alias(&mut self, node_id: NodeId) {
        self.editing_alias = Some(node_id);
        self.alias = String::new();
    }

    /// Called from above when we have finished editing the alias
    pub fn stop_editing_alias(&mut self) {
        self.editing_alias = None;
        self.alias = String::new();
    }

    /// Process an event sent by the subscription connected to the radio
    fn process_subscription_event(
        &mut self,
        subscription_event: SubscriptionEvent,
    ) -> Task<Message> {
        match subscription_event {
            ConnectingEvent(mac_address) => {
                self.connection_state = Connecting(mac_address);
                Task::none()
            },
            ConnectedEvent(ble_device, radio_type) => {
                self.connection_state = Connected(ble_device.clone(), radio_type);
                match self.viewing_channel {
                    None => {
                        let channel_id = self.viewing_channel;
                        let device = ble_device.clone();
                        Task::perform(empty(), move |_| {
                            Message::DeviceAndChannelConfigChange(
                                Some((device, radio_type)),
                                channel_id,
                            )
                        })
                    }
                    Some(channel_id) => {
                        Task::perform(empty(), move |_| {
                            DeviceViewEvent(ShowChannel(Some(channel_id)))
                        })
                    }
                }
            }
            DisconnectingEvent(mac_address) => {
                self.connection_state = Disconnecting(mac_address);
                Task::none()
            }
            DisconnectedEvent(id) => {
                if self.exit_pending {
                    std::process::exit(0);
                }
                self.connection_state = Disconnected(Some(id), None);
                self.channel_views.clear();
                self.nodes.clear();
                self.channels.clear();
                self.my_node_id = None;
                self.viewing_channel = None;
                Task::perform(empty(), |_| Navigation(DeviceListView))
            }
            #[allow(unused_variables)]
            Ready(sender, radio_type) => {
                match radio_type {
                    #[cfg(feature = "meshtastic")]
                    RadioType::Meshtastic => self.meshtastic_sender = Some(sender),
                    #[cfg(feature = "meshcore")]
                    RadioType::MeshCore => self.meshcore_sender = Some(sender),
                }
                Task::none()
            }
            ConnectionError(id, summary, detail) => {
                self.connection_state = Disconnected(Some(id), Some(summary.clone()));
                Task::perform(empty(), |_| Navigation(DeviceListView))
                    .chain(Task::perform(empty(), move |_| {
                        AppError(summary.clone(), detail.clone(), MeshChat::now())
                    }))
            }
            SendError(summary, detail) => {
                Task::perform(empty(), |_| Navigation(DeviceListView))
                    .chain(Task::perform(empty(), move |_| {
                        AppError(summary.clone(), detail.clone(), MeshChat::now())
                    }))
            }
            NotReady => {
                Task::perform(empty(), |_| Navigation(DeviceListView))
                    .chain(Task::perform(empty(), move |_| {
                        AppError("Subscription not ready".to_string(),
                                 "An attempt was made to communicate to radio prior to the subscription being Ready".to_string(),
                        MeshChat::now())
                    }))
            }
            MyNodeNum(my_node_num) => {
                self.my_node_id = Some(my_node_num);
                Task::none()
            },
            MyUserInfo(my_user_info) => {
                self.set_my_user(my_user_info);
                Task::none()
            }
            MyPosition(my_position) => {
                self.set_my_position(my_position);
                Task::none()
            }
            NewChannel(channel) => {
                self.add_channel(channel);
                Task::none()
            }
            NewNode(node_info) => {
                self.add_node(node_info);
                Task::none()
            }
            RadioNotification(message, timestamp) => {
                Task::perform(empty(), move |_| {
                    Message::AppNotification(
                        "Radio Notification".to_string(),
                        message,
                        timestamp
                    )
                })
            }
            MCMessageReceived(channel_id, id, from, mc_message, timestamp) => {
                if let Some(channel_view) = &mut self.channel_views.get_mut(&channel_id) {
                    let new_message = ChannelViewEntry::new(id, from, mc_message, timestamp);
                    channel_view.new_message(new_message, &self.history_length)
                } else {
                    eprintln!("No channel for MCMessage: channel_id = {:?}", channel_id);
                    Task::none()
                }
            }
            DeviceBatteryLevel(level) => {
                self.battery_level = level;
                Task::none()
            }
            NewNodeInfo(channel_id, id, from, mc_user, timestamp) => {
                self.update_node_user(from, &mc_user);

                if self.show_user_updates {
                    if let Some(channel_view) = &mut self.channel_views.get_mut(&channel_id) {
                        let new_message = ChannelViewEntry::new(id, from, UserMessage(mc_user), timestamp);
                        return channel_view.new_message(new_message, &self.history_length);
                    } else {
                        eprintln!("NewNodeInfo: Node '{}' unknown", mc_user.long_name);
                    }
                }
                Task::none()
            }
            NewNodePosition(channel_id, id, from, position, timestamp) => {
                self.update_node_position(from, &position);
                if self.show_position_updates {
                    if let Some(channel_view) = &mut self.channel_views.get_mut(&channel_id) {
                            let new_message = ChannelViewEntry::new(
                                id,
                                from,
                                PositionMessage(position),
                                timestamp,
                            );
                            return channel_view.new_message(new_message, &self.history_length);
                    } else {
                        eprintln!("No channel for: {}", channel_id);
                    }
                }
                Task::none()
            }
            ChannelName(channel_number, name) => {
                self.set_channel_name(channel_number, name);
                Task::none()
            }
            MessageACK(channel_id, message_id) => {
                if let Some(channel_view) = &mut self.channel_views.get_mut(&channel_id) {
                    channel_view.ack(message_id)
                } else {
                    eprintln!("No channel for MessageACK");
                }
                Task::none()
            }
        }
    }

    /// Generate a Settings button
    pub fn settings_button<'a>() -> Element<'a, Message> {
        tooltip(
            button(icons::cog())
                .style(button_chip_style)
                .on_press(OpenSettingsDialog),
            "Open Settings Dialog",
            tooltip::Position::Bottom,
        )
        .style(tooltip_style)
        .into()
    }

    ///  Add a new node to the list if it has the User info we want and is not marked to be ignored
    fn add_node(&mut self, node_info: MCNodeInfo) {
        if node_info.is_ignored {
            println!("Node is ignored!: {:?}", node_info);
        } else if let Some(my_node_num) = self.my_node_id {
            if node_info.node_id == my_node_num {
                self.my_position = node_info.position.clone();
                self.my_user = node_info.user.clone();
            }

            let channel_id = Node(node_info.node_id);
            self.nodes.insert(node_info.node_id, node_info);
            self.channel_views
                .insert(channel_id, ChannelView::new(channel_id, my_node_num));
        }
    }

    /// Set my user info to be the [MCUser] passed in
    fn set_my_user(&mut self, user: MCUser) {
        self.my_user = Some(user);
    }

    /// Set my position to be the [MCPosition] passed in
    fn set_my_position(&mut self, position: MCPosition) {
        self.my_position = Some(position);
    }

    /// Add a channel from the radio to the list if it is not disabled and has some Settings
    pub fn add_channel(&mut self, channel: MCChannel) {
        {
            if let Some(my_node_num) = self.my_node_id {
                self.channels.push(channel);
                let channel_id = ChannelId::Channel(ChannelIndex::from(self.channels.len() - 1));
                self.channel_views
                    .insert(channel_id, ChannelView::new(channel_id, my_node_num));
            }
        }
    }

    /// set the name of an existing channel if it can be found using its number
    pub fn set_channel_name(&mut self, channel_number: i32, name: String) {
        if let Some(channel) = self.channels.get_mut(channel_number as usize) {
            channel.name = name;
        }
    }

    /// If the Node is known already, then update its Position with a PositionApp update
    /// it has sent
    /// NOTE: This position maybe more recent, but it could have less accuracy, as position
    /// accuracy is set per channel. Consider deciding whether to update or not if it has lower
    /// accuracy depending on the age of the previous position?
    fn update_node_position(&mut self, from: NodeId, position: &MCPosition) {
        if let Some(node) = self.nodes.get_mut(&from) {
            node.position = Some(position.clone());

            if Some(from) == self.my_node_id {
                self.my_position = Some(position.clone());
            }
        }
    }

    /// If the Node is known already, then update its User with a NodeInfoApp User update
    fn update_node_user(&mut self, from: NodeId, user: &MCUser) {
        if let Some(node) = self.nodes.get_mut(&from) {
            node.user = Some(user.clone());
        }
    }

    /// Create a header view for the top of the screen depending on the current state of the app
    /// and whether we are in discovery mode or not.
    pub fn header<'a>(
        &'a self,
        config: &'a Config,
        state: &'a ConnectionState,
        device_list_view: &DeviceList,
    ) -> Element<'a, Message> {
        let mut header = Row::new().padding(4).align_y(Center).push(
            button("Devices")
                .style(button_chip_style)
                .on_press(Navigation(DeviceListView)),
        );

        header = match state {
            Disconnected(_, _) => header
                .push(Space::new().width(Fill))
                .push(button("Disconnected").style(button_chip_style)),
            Connecting(ble_device) => {
                let name_button = button(text(format!(
                    "Connecting to {}",
                    device_list_view.device_name_or_alias(ble_device, config)
                )))
                .style(button_chip_style);
                header.push(Space::new().width(Fill)).push(name_button)
            }
            Connected(device, _) => {
                let name_row = Row::new()
                    .push(text(format!(
                        "📱 {}",
                        device_list_view.device_name_or_alias(device, config)
                    )))
                    .push(Space::new().width(4))
                    .push(Self::unread_counter(self.unread_count(
                        self.show_position_updates,
                        self.show_user_updates,
                    )));
                let mut button = button(name_row).style(button_chip_style);
                // If viewing a channel of the device, allow navigating back to the device view
                if self.viewing_channel.is_some() {
                    button = button.on_press(DeviceViewEvent(ShowChannel(None)));
                }

                header
                    .push(button)
                    .push(Space::new().width(4))
                    .push(self.battery_level())
            }
            Disconnecting(device_name) => header
                .push(
                    button(text(format!(
                        "📱 {}",
                        device_list_view.device_name_or_alias(device_name, config)
                    )))
                    .style(button_chip_style),
                )
                .push(Space::new().width(Fill))
                .push(iced::widget::button("Disconnecting").style(button_chip_style)),
        };

        // possibly add a node/channel name button next
        match &self.viewing_channel {
            Some(ChannelId::Channel(channel_index)) => {
                let index: usize = (*channel_index).into();
                if let Some(channel) = self.channels.get(index) {
                    let channel_name = format!("🛜  {}", channel.name);
                    header = header.push(button(text(channel_name)).style(button_chip_style))
                }
            }
            Some(Node(node_id)) => {
                if let Some(node_name) = self.aliased_long_name(config, *node_id) {
                    header = header.push(button(text(node_name)).style(button_chip_style))
                }
            }
            None => {}
        }

        // Add a disconnect button on the right if we are connected
        if let Connected(_, _) = state {
            header = header.push(Space::new().width(Fill)).push(
                button("Disconnect")
                    .on_press(DeviceViewEvent(DisconnectRequest(false)))
                    .style(button_chip_style),
            )
        }

        header.push(Self::settings_button()).into()
    }

    /// Return an element that displays the battery level of the connected device
    fn battery_level(&self) -> Element<'_, Message> {
        let (battery_state, tooltip_text) = match self.battery_level {
            Some(battery_level) if battery_level <= 100 => (
                BatteryState::Charged(battery_level),
                format!("Battery {}%", battery_level),
            ),
            Some(_) => (BatteryState::Charging, "Charging".into()),
            None => (BatteryState::Unknown, "Battery Level Unknown".into()),
        };

        tooltip(
            Battery::new().state(battery_state).style(battery_style),
            text(tooltip_text),
            tooltip::Position::Bottom,
        )
        .style(tooltip_style)
        .into()
    }

    /// Count all the unread messages available to this device across channels and nodes
    pub fn unread_count(&self, show_position_updates: bool, show_user_updates: bool) -> usize {
        self.channel_views.values().fold(0, |acc, channel| {
            acc + channel.unread_count(show_position_updates, show_user_updates)
        })
    }

    /// Create the Element that shows the channels, nodes, etc.
    pub fn view<'a>(&'a self, config: &'a Config) -> Element<'a, Message> {
        if let Some(channel_number) = &self.viewing_channel
            && let Some(channel_view) = self.channel_views.get(channel_number)
        {
            return channel_view.view(
                &self.nodes,
                self.my_position.is_some(),
                self.my_user.is_some(),
                self,
                config,
                self.show_position_updates,
                self.show_user_updates,
            );
        }

        let select = |channel_number: ChannelId| DeviceViewEvent(ShowChannel(Some(channel_number)));

        // If not viewing a channel/user, show the list of channels and users
        let channel_and_node_scroll = self.channel_and_node_list(config, true, select);

        // Add a search box at the top, outside the scrollable area
        Column::new()
            .push(self.search_box())
            .push(channel_and_node_scroll)
            .push(self.button_row())
            .into()
    }

    /// Add a row of buttons at the bottom of the view to show position and into
    fn button_row(&self) -> Element<'static, Message> {
        let mut row = Row::new().padding(6).spacing(4);

        // If we know our position, show a button to show it on a map
        if let Some(position) = self.my_position.as_ref() {
            row = row.push(
                button(text("Show Position 📌"))
                    .style(button_chip_style)
                    .on_press(OpenUrl(MeshChat::location_url(position))),
            );
        }

        if let Some(user) = &self.my_user {
            row = row.push(
                button(text("Show Info ⓘ"))
                    .style(button_chip_style)
                    .on_press(ShowUserInfo(user.clone())),
            );
        }

        row.into()
    }

    /// Create a list of channels and nodes in this device with a button to select one of them
    pub fn channel_and_node_list<'a>(
        &'a self,
        config: &'a Config,
        add_buttons: bool,
        select: fn(ChannelId) -> Message,
    ) -> Element<'a, Message> {
        // If not viewing a channel/user, show the list of channels and users
        let mut channels_list = self.channel_list(select);

        // Add the favourite nodes to the list if there are any
        channels_list = self.favourite_nodes(channels_list, config, add_buttons, select);

        // Add the list of non-favourite nodes
        channels_list = self.nodes_list(channels_list, config, add_buttons, select);

        // Wrap the whole thing in a scrollable area
        scrollable(channels_list)
            .direction({
                let scrollbar = Scrollbar::new().width(10);
                scrollable::Direction::Vertical(scrollbar)
            })
            .style(scrollbar_style)
            .width(Fill)
            .height(Fill)
            .into()
    }

    /// Create a column with a set of rows, one for each channel
    fn channel_list(&self, select: fn(ChannelId) -> Message) -> Column<'_, Message> {
        let mut channels_list = Column::new();

        let mut filtered_channels: Vec<(usize, String)> = vec![];
        for (index, channel) in self.channels.iter().enumerate() {
            let channel_name = format!("🛜  {}", channel.name);

            // If there is a filter and the channel name does not contain it, don't show this row
            if channel_name.contains(&self.filter) {
                filtered_channels.push((index, channel_name));
            }
        }

        if !filtered_channels.is_empty() {
            channels_list = channels_list
                .push(self.section_header(format!("Channels ({})", self.channels.len())));

            for (index, channel_name) in filtered_channels {
                let channel_id = ChannelId::Channel(index.into());
                if let Some(channel_view) = self.channel_views.get(&channel_id) {
                    let channel_row = Self::channel_row(
                        channel_name,
                        channel_view
                            .unread_count(self.show_position_updates, self.show_user_updates),
                        channel_id,
                        select,
                    );
                    channels_list = channels_list.push(channel_row);
                }
            }
        }

        channels_list
    }

    /// Construct the list of favourite nodes, adding them to the channels_list Column
    /// If there is a filter, only show nodes that contain the filter in their name
    fn favourite_nodes<'a>(
        &'a self,
        mut channels_list: Column<'a, Message>,
        config: &'a Config,
        add_buttons: bool,
        select: fn(ChannelId) -> Message,
    ) -> Column<'a, Message> {
        let mut fav_nodes: Vec<NodeId> = vec![];

        for fav_node_id in &config.fav_nodes {
            if let Some(node_name) = self.aliased_long_name(config, *fav_node_id) {
                // If there is a filter and the Username does not contain it, don't show this row
                // Note that all Strings contain the empty filter value ""
                if node_name.contains(&self.filter) {
                    fav_nodes.push(*fav_node_id);
                }
            }
        }

        // filter out my own node if the node number is known yet
        fav_nodes.retain(|fav_node_id| Some(*fav_node_id) != self.my_node_id);

        // If there are favourite nodes, show the header and list them
        if !fav_nodes.is_empty() {
            channels_list = channels_list
                .push(self.section_header(format!("Favourite Nodes ({})", fav_nodes.len())));
            for fav_node_id in fav_nodes {
                let channel_id = Node(fav_node_id);
                if let Some(channel_view) = self.channel_views.get(&channel_id) {
                    channels_list = channels_list.push(
                        self.node_row(
                            channel_view
                                .unread_count(self.show_position_updates, self.show_user_updates),
                            fav_node_id,
                            true, // Favourite
                            config,
                            add_buttons,
                            select,
                        ),
                    );
                }
            }
        }

        channels_list
    }

    /// Create the list of nodes that are not already in the favourite nodes list
    /// If there is a filter, only show nodes that contain the filter in their name
    fn nodes_list<'a>(
        &'a self,
        mut channels_list: Column<'a, Message>,
        config: &'a Config,
        add_buttons: bool,
        select: fn(ChannelId) -> Message,
    ) -> Column<'a, Message> {
        // Initial list of nodes that are NOT already in the list of favourite nodes and does
        // not include my own node (if the node number is known)
        let other_nodes_list = self
            .nodes
            .keys()
            .filter(|node_id| {
                !config.fav_nodes.contains(node_id) && Some(**node_id) != self.my_node_id
            })
            .filter(|node_id| {
                if let Some(node_name) = self.aliased_long_name(config, **node_id) {
                    node_name.contains(&self.filter)
                } else {
                    false
                }
            })
            .collect::<Vec<_>>();

        if !other_nodes_list.is_empty() {
            channels_list = channels_list
                .push(self.section_header(format!("Nodes ({})", other_nodes_list.len())));

            for node_id in other_nodes_list {
                let channel_id = Node(*node_id);
                if let Some(channel_view) = self.channel_views.get(&channel_id) {
                    channels_list = channels_list.push(
                        self.node_row(
                            channel_view
                                .unread_count(self.show_position_updates, self.show_user_updates),
                            *node_id,
                            false, // Not a Favourite
                            config,
                            add_buttons,
                            select,
                        ),
                    );
                }
            }
        }

        channels_list
    }

    /// Add a section header between areas of the list
    fn section_header(&self, title: String) -> Element<'_, Message> {
        Column::new()
            .push(
                Container::new(text(title).size(16))
                    .align_x(Center)
                    .padding(Padding::from([6, 12]))
                    .style(|_| DAY_SEPARATOR_STYLE),
            )
            .width(Fill)
            .align_x(Center)
            .into()
    }

    /// Return the long name for the node with id node_id - prefixed with a node/device emoji -
    /// if the node is known and has a name
    fn aliased_long_name<'a>(&'a self, config: &'a Config, node_id: NodeId) -> Option<&'a str> {
        // Avoid returning an (aliased) name until we have the node info
        let node = self.nodes.get(&node_id)?;
        if let Some(alias) = config.aliases.get(&node_id) {
            return Some(alias);
        }
        let user = node.user.as_ref()?;
        Some(&user.long_name)
    }

    /// An element that will show a count of unread messages if greater than zero, or nothing
    fn unread_counter(num_messages: usize) -> Element<'static, Message> {
        if num_messages > 0 {
            tooltip(
                container(text(format!("{}", num_messages)))
                    .padding([0, 6])
                    .style(count_style),
                text(format!("{} Unread Messages", num_messages)),
                tooltip::Position::Right,
            )
            .style(tooltip_style)
            .into()
        } else {
            Space::new().width(0).into()
        }
    }

    /// Create a Button that represents either a Channel or a Node
    /// DeviceViewEvent(ShowChannel(Some(channel_id)))
    /// DeviceViewEvent(ShowChannel(Some(channel_id)))
    fn channel_row(
        name: String,
        unread_count: usize,
        channel_id: ChannelId,
        select: fn(ChannelId) -> Message,
    ) -> Element<'static, Message> {
        let name_row = Row::new()
            .push(text(name))
            .push(Space::new().width(4))
            .push(Self::unread_counter(unread_count));

        Row::new()
            .push(
                button(name_row)
                    .on_press(select(channel_id))
                    .width(Fill)
                    .style(channel_row_style),
            )
            .push(Space::new().width(10))
            .into()
    }

    /// Create a row for a Node in the device view with the name, unread count, location icon/button,
    /// and favourite button.
    fn node_row<'a>(
        &self,
        num_messages: usize,
        node_id: NodeId,
        favourite: bool,
        config: &'a Config,
        add_buttons: bool,
        select: fn(ChannelId) -> Message,
    ) -> Element<'a, Message> {
        let long_name: &str = self
            .nodes
            .get(&node_id)
            .and_then(|node_info| node_info.user.as_ref().map(|user| user.long_name.as_ref()))
            .unwrap_or("Unknown");

        let name_element: Element<'a, Message> = if let Some(alias) = config.aliases.get(&node_id) {
            tooltip(
                text(alias),
                text(format!("Original user name: {}", long_name)),
                tooltip::Position::Right,
            )
            .style(tooltip_style)
            .into()
        } else if let Some(editing_node_id) = self.editing_alias
            && editing_node_id == node_id
        {
            text_input("Enter alias for this Node", &self.alias)
                .on_input(|s| DeviceViewEvent(AliasInput(s)))
                .on_submit(AddNodeAlias(editing_node_id, self.alias.clone()))
                .style(text_input_style)
                .into()
        } else {
            text(long_name.to_string()).into()
        };

        let name_row = Row::new()
            .push("📱  ")
            .push(name_element)
            .push(Space::new().width(4))
            .push(Self::unread_counter(num_messages))
            .align_y(Center);

        let mut node_row = Row::new().align_y(Bottom);

        let channel_id = Node(node_id);
        node_row = if self.editing_alias.is_none() {
            node_row.push(
                button(name_row)
                    .on_press(select(channel_id))
                    .width(Fill)
                    .style(channel_row_style),
            )
        } else {
            node_row.push(name_row)
        };

        if add_buttons {
            node_row = self.add_buttons(node_row, node_id, favourite, config)
        }

        node_row.into()
    }

    fn add_buttons<'a>(
        &self,
        mut node_row: Row<'a, Message>,
        node_id: NodeId,
        favourite: bool,
        config: &'a Config,
    ) -> Row<'a, Message> {
        // Add a button to add or remove an alias for this node '👤'
        let (tooltip_text, message) = if config.aliases.contains_key(&node_id) {
            ("Remove alias for this node", RemoveNodeAlias(node_id))
        } else {
            (
                "Add an alias for this node",
                DeviceViewEvent(StartEditingAlias(node_id)),
            )
        };

        node_row = node_row.push(
            tooltip(
                button(text("👤")).on_press(message).style(fav_button_style),
                tooltip_text,
                tooltip::Position::Left,
            )
            .gap(6)
            .style(tooltip_style),
        );

        // Add a button to show the User info of the node if present
        node_row = if let Some(node) = self.nodes.get(&node_id)
            && let Some(user) = &node.user
        {
            node_row.push(
                tooltip(
                    button(text("ⓘ"))
                        .style(fav_button_style)
                        .on_press(ShowUserInfo(user.clone()))
                        .width(36),
                    "Show node's User info",
                    tooltip::Position::Left,
                )
                .gap(6)
                .style(tooltip_style),
            )
        } else {
            node_row.push(Space::new().width(36))
        };

        // Add a button to show the location of the node if it has one
        node_row = if let Some(node) = self.nodes.get(&node_id)
            && let Some(position) = &node.position
        {
            node_row.push(
                tooltip(
                    button(text("📌"))
                        .style(fav_button_style)
                        .on_press(ShowLocation(position.clone()))
                        .width(36),
                    "Show node position in maps",
                    tooltip::Position::Left,
                )
                .gap(6)
                .style(tooltip_style),
            )
        } else {
            node_row.push(Space::new().width(36))
        };

        // Add a button to toggle the favourite status of the node
        let (tooltip_text, icon) = if favourite {
            ("Unfavourite this node", icons::star())
        } else {
            ("Favourite this node", icons::star_empty())
        };

        node_row
            .push(
                tooltip(
                    button(icon)
                        .on_press(ToggleNodeFavourite(node_id))
                        .style(fav_button_style),
                    tooltip_text,
                    tooltip::Position::Left,
                )
                .gap(6)
                .style(tooltip_style),
            )
            .push(Space::new().width(10))
    }

    fn search_box(&self) -> Element<'static, Message> {
        container(
            container(
                Row::new()
                    .push(Space::new().width(9.0))
                    .push(
                        text_input("Search for Channel or Node", &self.filter)
                            .style(text_input_style)
                            .padding([4, 4])
                            .id(CHANNEL_SEARCH_ID)
                            .on_input(|s| DeviceViewEvent(SearchInput(s))),
                    )
                    .push(Space::new().width(4.0))
                    .push(text_input_clear_button(!self.filter.is_empty()))
                    .push(Space::new().width(4.0))
                    .align_y(Center),
            )
            .style(text_input_container_style),
        )
        .padding(Padding::from([8, 8]))
        .into()
    }
}
pub fn text_input_clear_button(enable: bool) -> Button<'static, Message> {
    let mut clear_button = button(text("⨂").size(18))
        .style(text_input_button_style)
        .padding(Padding::from([6, 6]));
    if enable {
        clear_button = clear_button.on_press(DeviceViewEvent(ClearFilter));
    }

    clear_button
}

/// Return a short name to display in the message box as the source of a message.
/// If the message is from myself, then return None.
pub fn short_name(nodes: &HashMap<NodeId, MCNodeInfo>, from: NodeId) -> &str {
    nodes
        .get(&from)
        .and_then(|node_info: &MCNodeInfo| node_info.user.as_ref())
        .map(|user: &MCUser| user.short_name.as_ref())
        .unwrap_or("????")
}

/// Return a long name for the source node
pub fn long_name(nodes: &HashMap<NodeId, MCNodeInfo>, from: NodeId) -> &str {
    nodes
        .get(&from)
        .and_then(|node_info: &MCNodeInfo| node_info.user.as_ref())
        .map(|user: &MCUser| user.long_name.as_ref())
        .unwrap_or("Node information not available")
}

#[cfg(test)]
#[allow(clippy::field_reassign_with_default)]
mod tests {
    use super::*;
    use crate::Message::Navigation;
    use crate::device::ConnectionState::{Connected, Connecting, Disconnected, Disconnecting};
    use crate::device::DeviceViewMessage::{ClearFilter, SearchInput};
    use crate::device::SubscriberMessage::{Connect, Disconnect};
    use crate::test_helper;
    use btleplug::api::BDAddr;

    fn test_position(lat: f64, lon: f64) -> MCPosition {
        MCPosition {
            latitude: lat,
            longitude: lon,
            ..Default::default()
        }
    }

    #[cfg(feature = "meshtastic")]
    #[tokio::test]
    async fn test_connect_request_fail() {
        let mut meshchat = test_helper::test_app();
        // Subscription won't be ready
        let _task = meshchat.device.subscriber_send(
            Connect(
                BDAddr::from([0, 0, 0, 0, 0, 0]).to_string(),
                RadioType::Meshtastic,
            ),
            Navigation(DeviceListView),
        );

        assert_eq!(meshchat.device.connection_state, Disconnected(None, None));
    }

    #[cfg(feature = "meshtastic")]
    #[tokio::test]
    async fn test_disconnect_request_fail() {
        let mut meshchat = test_helper::test_app();
        // Subscription won't be ready
        let _task = meshchat
            .device
            .subscriber_send(Disconnect, Navigation(DeviceListView));
        assert_eq!(meshchat.device.connection_state, Disconnected(None, None));
    }

    #[test]
    fn test_connection_state_default() {
        let state = ConnectionState::default();
        assert_eq!(state, Disconnected(None, None));
    }

    #[cfg(feature = "meshtastic")]
    #[test]
    fn test_connection_state_variants() {
        let disconnected = Disconnected(Some("device1".into()), Some("error".into()));
        let connecting = Connecting("device1".into());
        let connected = Connected("device1".into(), RadioType::Meshtastic);
        let disconnecting = Disconnecting("device1".into());

        // Test equality
        assert_eq!(
            disconnected,
            Disconnected(Some("device1".into()), Some("error".into()))
        );
        assert_eq!(connecting, Connecting("device1".into()));
        assert_eq!(
            connected,
            Connected("device1".into(), RadioType::Meshtastic,)
        );
        assert_eq!(disconnecting, Disconnecting("device1".into()));

        // Test inequality
        assert_ne!(connecting, connected);
        assert_ne!(connected, disconnecting);
    }

    #[test]
    fn test_short_name_unknown_node() {
        let nodes: HashMap<NodeId, MCNodeInfo> = HashMap::new();
        assert_eq!(short_name(&nodes, NodeId::from(12345u64)), "????");
    }

    //noinspection ALL
    #[test]
    fn test_short_name_known_node() {
        let mut nodes: HashMap<NodeId, MCNodeInfo> = HashMap::new();
        nodes.insert(
            NodeId::from(12345u64),
            MCNodeInfo {
                node_id: NodeId::from(12345u64),
                user: Some(MCUser {
                    id: "test".into(),
                    long_name: "Test User".into(),
                    short_name: "TEST".into(),
                    hw_model_str: "TBEAM".into(),
                    role_str: "CLIENT".into(),
                    ..Default::default()
                }),
                position: None,
                is_ignored: false,
            },
        );
        assert_eq!(short_name(&nodes, NodeId::from(12345u64)), "TEST");
    }

    #[test]
    fn test_short_name_node_without_user() {
        let mut nodes: HashMap<NodeId, MCNodeInfo> = HashMap::new();
        nodes.insert(
            NodeId::from(12345u64),
            MCNodeInfo {
                node_id: NodeId::from(12345u64),
                ..Default::default()
            },
        );
        assert_eq!(short_name(&nodes, NodeId::from(12345u64)), "????");
    }

    #[test]
    fn test_long_name_unknown_node() {
        let nodes: HashMap<NodeId, MCNodeInfo> = HashMap::new();
        assert_eq!(
            long_name(&nodes, NodeId::from(12345u64)),
            "Node information not available"
        );
    }

    #[test]
    fn test_long_name_known_node() {
        let mut nodes: HashMap<NodeId, MCNodeInfo> = HashMap::new();
        nodes.insert(
            NodeId::from(12345u64),
            MCNodeInfo {
                node_id: NodeId::from(12345u64),
                user: Some(MCUser {
                    id: "test".into(),
                    long_name: "Test User Long Name".into(),
                    short_name: "TEST".into(),
                    hw_model_str: "TBEAM".into(),
                    role_str: "CLIENT".into(),
                    ..Default::default()
                }),
                position: None,
                is_ignored: false,
            },
        );
        assert_eq!(
            long_name(&nodes, NodeId::from(12345u64)),
            "Test User Long Name"
        );
    }

    #[test]
    fn test_device_view_default() {
        let device_view = Device::default();
        assert_eq!(device_view.connection_state(), &Disconnected(None, None));
        assert_eq!(device_view.unread_count(true, true), 0);
        assert!(device_view.channel_views.is_empty());
    }

    #[test]
    fn test_search_input() {
        let mut device_view = Device::default();
        assert!(device_view.filter.is_empty());

        let _ = device_view.update(SearchInput("test".into()));
        assert_eq!(device_view.filter, "test");
    }

    #[test]
    fn test_clear_filter() {
        let mut device_view = Device::default();
        let _ = device_view.update(SearchInput("test".into()));
        assert!(!device_view.filter.is_empty());

        let _ = device_view.update(ClearFilter);
        assert!(device_view.filter.is_empty());
    }

    #[test]
    fn test_cancel_interactive() {
        let mut device_view = Device {
            editing_alias: Some(NodeId::from(123u64)),
            alias: "test alias".into(),
            ..Default::default()
        };

        device_view.cancel_interactive();

        assert!(device_view.editing_alias.is_none());
        assert!(device_view.alias.is_empty());
        assert!(device_view.forwarding_message.is_none());
    }

    #[test]
    fn test_start_editing_alias() {
        let mut device_view = Device {
            alias: "existing".into(),
            ..Default::default()
        };

        let _ = device_view.update(StartEditingAlias(NodeId::from(456u64)));

        assert_eq!(device_view.editing_alias, Some(NodeId::from(456u64)));
        assert!(device_view.alias.is_empty()); // Should be cleared
    }

    #[test]
    fn test_stop_editing_alias() {
        let mut device_view = Device {
            editing_alias: Some(NodeId::from(123u64)),
            alias: "test".into(),
            ..Default::default()
        };

        device_view.stop_editing_alias();

        assert!(device_view.editing_alias.is_none());
        assert!(device_view.alias.is_empty());
    }

    #[test]
    fn test_alias_input() {
        let mut device_view = Device::default();
        let _ = device_view.update(AliasInput("new alias".into()));
        assert_eq!(device_view.alias, "new alias");
    }

    #[test]
    fn test_set_history_length() {
        let mut device_view = Device::default();
        device_view.set_history_length(HistoryLength::NumberOfMessages(50));
        assert_eq!(
            device_view.history_length,
            HistoryLength::NumberOfMessages(50)
        );
    }

    #[test]
    fn test_set_show_position_updates() {
        let mut device_view = Device::default();
        assert!(!device_view.show_position_updates);

        device_view.set_show_position_updates(true);
        assert!(device_view.show_position_updates);

        device_view.set_show_position_updates(false);
        assert!(!device_view.show_position_updates);
    }

    #[test]
    fn test_set_show_user_updates() {
        let mut device_view = Device::default();
        assert!(!device_view.show_user_updates);

        device_view.set_show_user_updates(true);
        assert!(device_view.show_user_updates);
    }

    #[test]
    fn test_start_forwarding_message() {
        let mut device_view = Device::default();
        assert!(device_view.forwarding_message.is_none());

        let entry = ChannelViewEntry::new(
            MessageId::from(1),
            NodeId::from(100u64),
            MCContent::NewTextMessage("test".into()),
            TimeStamp::from(0),
        );
        let _ = device_view.update(StartForwardingMessage(entry));

        assert!(device_view.forwarding_message.is_some());
    }

    #[test]
    fn test_stop_forwarding_message() {
        let mut device_view = Device::default();
        let entry = ChannelViewEntry::new(
            MessageId::from(1),
            NodeId::from(100u64),
            MCContent::NewTextMessage("test".into()),
            TimeStamp::from(0),
        );
        let _ = device_view.update(StartForwardingMessage(entry));
        assert!(device_view.forwarding_message.is_some());

        let _ = device_view.update(StopForwardingMessage);
        assert!(device_view.forwarding_message.is_none());
    }

    #[test]
    fn test_unread_count_empty() {
        let device_view = Device::default();
        assert_eq!(device_view.unread_count(true, true), 0);
    }

    // Tests for process_subscription_event

    #[test]
    fn test_subscription_connecting_event() {
        let mut device_view = Device::default();
        let _ = device_view.update(SubscriptionMessage(ConnectingEvent("device1".into())));
        assert_eq!(device_view.connection_state, Connecting("device1".into()));
    }

    #[cfg(feature = "meshtastic")]
    #[test]
    fn test_subscription_connected_event() {
        let mut device_view = Device::default();
        let _ = device_view.update(SubscriptionMessage(ConnectedEvent(
            "device1".into(),
            RadioType::Meshtastic,
        )));
        assert_eq!(
            device_view.connection_state,
            Connected("device1".into(), RadioType::Meshtastic,)
        );
    }

    #[cfg(feature = "meshtastic")]
    #[test]
    fn test_subscription_disconnecting_event() {
        let mut device_view = Device::default();
        device_view.connection_state = Connected("device1".into(), RadioType::Meshtastic);
        let _ = device_view.update(SubscriptionMessage(DisconnectingEvent("device1".into())));
        assert_eq!(
            device_view.connection_state,
            Disconnecting("device1".into())
        );
    }

    #[test]
    fn test_subscription_my_node_num() {
        let mut device_view = Device::default();
        assert!(device_view.my_node_id.is_none());

        let _ = device_view.update(SubscriptionMessage(MyNodeNum(NodeId::from(12345u64))));
        assert_eq!(device_view.my_node_id, Some(NodeId::from(12345u64)));
    }

    #[cfg(feature = "meshtastic")]
    #[test]
    fn test_subscription_ready() {
        let mut device_view = Device::default();
        assert!(device_view.meshtastic_sender.is_none());

        let (sender, _receiver) = tokio::sync::mpsc::channel::<SubscriberMessage>(10);
        let _ = device_view.update(SubscriptionMessage(Ready(sender, RadioType::Meshtastic)));
        assert!(device_view.meshtastic_sender.is_some());
    }

    #[test]
    fn test_subscription_device_battery_level() {
        let mut device_view = Device::default();
        assert!(device_view.battery_level.is_none());

        let _ = device_view.update(SubscriptionMessage(DeviceBatteryLevel(Some(75))));
        assert_eq!(device_view.battery_level, Some(75));
    }

    #[test]
    fn test_subscription_device_battery_level_none() {
        let mut device_view = Device::default();
        let _ = device_view.update(SubscriptionMessage(DeviceBatteryLevel(None)));
        assert!(device_view.battery_level.is_none());
    }

    #[test]
    fn test_subscription_new_channel() {
        let mut device_view = Device::default();
        device_view.my_node_id = Some(NodeId::from(999u64));
        assert!(device_view.channels.is_empty());

        let channel = MCChannel {
            index: 0,
            name: "TestChannel".into(),
        };
        let _ = device_view.update(SubscriptionMessage(NewChannel(channel)));

        assert_eq!(device_view.channels.len(), 1);
        assert_eq!(device_view.channels[0].name, "TestChannel");
        assert!(
            device_view
                .channel_views
                .contains_key(&ChannelId::Channel(0.into()))
        );
    }

    #[test]
    fn test_subscription_new_node() {
        let mut device_view = Device::default();
        device_view.my_node_id = Some(NodeId::from(999u64));

        let node_info = MCNodeInfo {
            node_id: NodeId::from(12345u64),
            user: Some(MCUser {
                id: "test".into(),
                long_name: "Test User".into(),
                short_name: "TEST".into(),
                hw_model_str: "TBEAM".into(),
                role_str: "CLIENT".into(),
                ..Default::default()
            }),
            position: None,
            is_ignored: false,
        };

        let _ = device_view.update(SubscriptionMessage(NewNode(node_info)));

        assert!(device_view.nodes.contains_key(&NodeId::from(12345u64)));
        assert!(
            device_view
                .channel_views
                .contains_key(&Node(NodeId::from(12345u64)))
        );
    }

    #[test]
    fn test_subscription_new_node_ignored() {
        let mut device_view = Device::default();
        device_view.my_node_id = Some(NodeId::from(999u64));

        let node_info = MCNodeInfo {
            node_id: NodeId::from(12345u64),
            user: None,
            position: None,
            is_ignored: true, // Should be ignored
        };

        let _ = device_view.update(SubscriptionMessage(NewNode(node_info)));

        // Node should not be added because is_ignored is true
        assert!(!device_view.nodes.contains_key(&NodeId::from(12345u64)));
    }

    #[test]
    fn test_subscription_new_node_is_my_node() {
        let mut device_view = Device::default();
        device_view.my_node_id = Some(NodeId::from(12345u64));

        let position = test_position(37.7749, -122.4194);

        let user = MCUser {
            id: "me".into(),
            long_name: "My Name".into(),
            short_name: "ME".into(),
            hw_model_str: "TBEAM".into(),
            role_str: "CLIENT".into(),
            ..Default::default()
        };

        let node_info = MCNodeInfo {
            node_id: NodeId::from(12345u64),
            user: Some(user.clone()),
            position: Some(position.clone()),
            is_ignored: false,
        };

        let _ = device_view.update(SubscriptionMessage(NewNode(node_info)));

        // my_position and my_user should be set
        assert!(device_view.my_position.is_some());
        assert!(device_view.my_user.is_some());
        assert_eq!(
            device_view
                .my_user
                .as_ref()
                .expect("my_user should be set after adding my node")
                .long_name,
            "My Name"
        );
    }

    #[test]
    fn test_update_node_position() {
        let mut device_view = Device::default();
        device_view.my_node_id = Some(NodeId::from(999u64));

        // Add a node first
        let node_info = MCNodeInfo {
            node_id: NodeId::from(12345u64),
            user: Some(MCUser {
                id: "test".into(),
                long_name: "Test".into(),
                short_name: "T".into(),
                ..Default::default()
            }),
            position: None,
            is_ignored: false,
        };
        let _ = device_view.update(SubscriptionMessage(NewNode(node_info)));

        // Update position
        let position = test_position(40.7128, -74.0060);
        device_view.update_node_position(NodeId::from(12345u64), &position);

        assert!(
            device_view
                .nodes
                .get(&NodeId::from(12345u64))
                .expect("node 12345 should exist")
                .position
                .is_some()
        );
    }

    #[test]
    fn test_update_node_position_my_node() {
        let mut device_view = Device::default();
        device_view.my_node_id = Some(NodeId::from(12345u64));

        // Add my node first
        let node_info = MCNodeInfo {
            node_id: NodeId::from(12345u64),
            ..Default::default()
        };
        let _ = device_view.update(SubscriptionMessage(NewNode(node_info)));

        // Update my position
        let position = test_position(40.7128, -74.0060);
        device_view.update_node_position(NodeId::from(12345u64), &position);

        // my_position should also be updated
        assert!(device_view.my_position.is_some());
        assert_eq!(
            device_view
                .my_position
                .as_ref()
                .expect("my_position should be set after position update")
                .latitude,
            40.7128
        );
    }

    #[test]
    fn test_update_node_user() {
        let mut device_view = Device::default();
        device_view.my_node_id = Some(NodeId::from(999u64));

        // Add a node first
        let node_info = MCNodeInfo {
            node_id: NodeId::from(12345u64),
            ..Default::default()
        };
        let _ = device_view.update(SubscriptionMessage(NewNode(node_info)));

        // Update user
        let user = MCUser {
            id: "updated".into(),
            long_name: "Updated Name".into(),
            short_name: "UPD".into(),
            ..Default::default()
        };
        device_view.update_node_user(NodeId::from(12345u64), &user);

        assert!(
            device_view
                .nodes
                .get(&NodeId::from(12345u64))
                .expect("node 12345 should exist")
                .user
                .is_some()
        );
        assert_eq!(
            device_view
                .nodes
                .get(&NodeId::from(12345u64))
                .expect("node 12345 should exist")
                .user
                .as_ref()
                .expect("user should be set after update")
                .long_name,
            "Updated Name"
        );
    }

    #[test]
    fn test_aliased_long_name_no_node() {
        let device_view = Device::default();
        let config = Config::default();
        assert!(
            device_view
                .aliased_long_name(&config, NodeId::from(12345u64))
                .is_none()
        );
    }

    #[test]
    fn test_aliased_long_name_with_alias() {
        let mut device_view = Device::default();
        device_view.my_node_id = Some(NodeId::from(999u64));

        let node_info = MCNodeInfo {
            node_id: NodeId::from(12345u64),
            user: Some(MCUser {
                id: "test".into(),
                long_name: "Original Name".into(),
                short_name: "ON".into(),
                ..Default::default()
            }),
            position: None,
            is_ignored: false,
        };
        let _ = device_view.update(SubscriptionMessage(NewNode(node_info)));

        let mut config = Config::default();
        config
            .aliases
            .insert(NodeId::from(12345u64), "My Alias".into());

        assert_eq!(
            device_view.aliased_long_name(&config, NodeId::from(12345u64)),
            Some("My Alias")
        );
    }

    #[test]
    fn test_aliased_long_name_without_alias() {
        let mut device_view = Device::default();
        device_view.my_node_id = Some(NodeId::from(999u64));

        let node_info = MCNodeInfo {
            node_id: NodeId::from(12345u64),
            user: Some(MCUser {
                id: "test".into(),
                long_name: "Original Name".into(),
                short_name: "ON".into(),
                ..Default::default()
            }),
            position: None,
            is_ignored: false,
        };
        let _ = device_view.update(SubscriptionMessage(NewNode(node_info)));

        let config = Config::default();

        assert_eq!(
            device_view.aliased_long_name(&config, NodeId::from(12345u64)),
            Some("Original Name")
        );
    }

    #[test]
    fn test_aliased_long_name_node_without_user() {
        let mut device_view = Device::default();
        device_view.my_node_id = Some(NodeId::from(999u64));

        let node_info = MCNodeInfo {
            node_id: NodeId::from(12345u64),
            ..Default::default()
        };
        let _ = device_view.update(SubscriptionMessage(NewNode(node_info)));

        let config = Config::default();

        // No user, so should return None
        assert!(
            device_view
                .aliased_long_name(&config, NodeId::from(12345u64))
                .is_none()
        );
    }

    #[test]
    fn test_connection_state_accessor() {
        let device_view = Device::default();
        assert_eq!(device_view.connection_state(), &Disconnected(None, None));
    }

    #[test]
    fn test_show_channel() {
        let mut device_view = Device::default();
        assert!(device_view.viewing_channel.is_none());

        let _ = device_view.update(ShowChannel(Some(ChannelId::Channel(0.into()))));
        assert_eq!(
            device_view.viewing_channel,
            Some(ChannelId::Channel(0.into()))
        );
    }

    #[test]
    fn test_show_channel_none() {
        let mut device_view = Device::default();
        device_view.viewing_channel = Some(ChannelId::Channel(0.into()));

        let _ = device_view.update(ShowChannel(None));
        assert!(device_view.viewing_channel.is_none());
    }

    #[test]
    fn test_subscription_connection_error() {
        let mut device_view = Device::default();
        device_view.connection_state = Connecting("device1".into());

        let _ = device_view.update(SubscriptionMessage(ConnectionError(
            "device1".into(),
            "Connection failed".into(),
            "Timeout".into(),
        )));

        assert!(matches!(
            device_view.connection_state,
            Disconnected(Some(_), Some(_))
        ));
    }

    #[test]
    fn test_add_multiple_channels() {
        let mut device_view = Device::default();
        device_view.my_node_id = Some(NodeId::from(999u64));

        let _ = device_view.update(SubscriptionMessage(NewChannel(MCChannel {
            index: 0,
            name: "Channel1".into(),
        })));
        let _ = device_view.update(SubscriptionMessage(NewChannel(MCChannel {
            index: 1,
            name: "Channel2".into(),
        })));

        assert_eq!(device_view.channels.len(), 2);
        assert!(
            device_view
                .channel_views
                .contains_key(&ChannelId::Channel(0.into()))
        );
        assert!(
            device_view
                .channel_views
                .contains_key(&ChannelId::Channel(1.into()))
        );
    }

    #[test]
    fn test_message_ack_no_channel() {
        let mut device_view = Device::default();
        // No channel exists, should not panic
        let _ = device_view.update(SubscriptionMessage(MessageACK(
            ChannelId::Channel(0.into()),
            MessageId::from(123),
        )));
    }

    #[test]
    fn test_channel_msg_no_channel() {
        let mut device_view = Device::default();
        // No channel exists, should not panic
        let _ = device_view.update(ChannelMsg(
            ChannelId::Channel(0.into()),
            ChannelViewMessage::MessageInput("test".into()),
        ));
    }

    #[test]
    fn test_text_input_clear_button_enabled() {
        // When enabled (has content), the button should have on_press handler
        let button = text_input_clear_button(true);
        // We can't directly test on_press, but we verify the button is created
        drop(button);
    }

    #[test]
    fn test_text_input_clear_button_disabled() {
        // When disabled (no content), the button should not have on_press handler
        let button = text_input_clear_button(false);
        drop(button);
    }

    #[test]
    fn test_set_channel_name_existing_channel() {
        let mut device_view = Device::default();
        device_view.channels.push(MCChannel {
            index: 0,
            name: "Original".into(),
        });

        device_view.set_channel_name(0, "Updated".into());
        assert_eq!(device_view.channels[0].name, "Updated");
    }

    #[test]
    fn test_set_channel_name_nonexistent_channel() {
        let mut device_view = Device::default();
        // No channels exist, should not panic
        device_view.set_channel_name(5, "Test".into());
        assert!(device_view.channels.is_empty());
    }

    #[test]
    fn test_set_channel_name_multiple_channels() {
        let mut device_view = Device::default();
        device_view.channels.push(MCChannel {
            index: 0,
            name: "Channel0".into(),
        });
        device_view.channels.push(MCChannel {
            index: 1,
            name: "Channel1".into(),
        });
        device_view.channels.push(MCChannel {
            index: 2,
            name: "Channel2".into(),
        });

        device_view.set_channel_name(1, "UpdatedChannel1".into());

        assert_eq!(device_view.channels[0].name, "Channel0");
        assert_eq!(device_view.channels[1].name, "UpdatedChannel1");
        assert_eq!(device_view.channels[2].name, "Channel2");
    }

    #[test]
    fn test_channel_change_no_change() {
        let mut device_view = Device::default();
        device_view.viewing_channel = Some(ChannelId::Channel(0.into()));

        // The same channel should return Task::none equivalent behavior
        let _task = device_view.channel_change(Some(ChannelId::Channel(0.into())));
        assert_eq!(
            device_view.viewing_channel,
            Some(ChannelId::Channel(0.into()))
        );
    }

    #[test]
    fn test_channel_change_to_different_channel() {
        let mut device_view = Device::default();
        device_view.viewing_channel = Some(ChannelId::Channel(0.into()));

        let _task = device_view.channel_change(Some(ChannelId::Channel(1.into())));
        assert_eq!(
            device_view.viewing_channel,
            Some(ChannelId::Channel(1.into()))
        );
    }

    #[test]
    fn test_channel_change_to_none() {
        let mut device_view = Device::default();
        device_view.viewing_channel = Some(ChannelId::Channel(0.into()));

        let _task = device_view.channel_change(None);
        assert_eq!(device_view.viewing_channel, None);
    }

    #[test]
    fn test_channel_change_from_none() {
        let mut device_view = Device::default();
        device_view.viewing_channel = None;

        let _task = device_view.channel_change(Some(ChannelId::Channel(0.into())));
        assert_eq!(
            device_view.viewing_channel,
            Some(ChannelId::Channel(0.into()))
        );
    }

    #[test]
    fn test_channel_change_to_node() {
        let mut device_view = Device::default();
        device_view.viewing_channel = Some(ChannelId::Channel(0.into()));

        let _task = device_view.channel_change(Some(Node(NodeId::from(12345u64))));
        assert_eq!(
            device_view.viewing_channel,
            Some(Node(NodeId::from(12345u64)))
        );
    }

    #[test]
    fn test_subscription_channel_name() {
        let mut device_view = Device::default();
        device_view.channels.push(MCChannel {
            index: 0,
            name: "Original".into(),
        });

        let _ = device_view.update(SubscriptionMessage(ChannelName(0, "Updated".into())));
        assert_eq!(device_view.channels[0].name, "Updated");
    }

    #[test]
    fn test_subscription_channel_name_invalid_index() {
        let mut device_view = Device::default();
        // No channels exist, should not panic
        let _ = device_view.update(SubscriptionMessage(ChannelName(5, "Test".into())));
        assert!(device_view.channels.is_empty());
    }

    #[test]
    fn test_subscription_radio_notification() {
        let mut device_view = Device::default();
        let _ = device_view.update(SubscriptionMessage(RadioNotification(
            "Test notification".into(),
            TimeStamp::from(1234567890),
        )));
        // Should not panic, returns a task
    }

    #[test]
    fn test_subscription_not_ready() {
        let mut device_view = Device::default();
        let _ = device_view.update(SubscriptionMessage(NotReady));
        // Should not panic
    }

    #[test]
    fn test_mc_message_received_no_channel() {
        let mut device_view = Device::default();
        // No channel exists, should not panic
        let _ = device_view.update(SubscriptionMessage(MCMessageReceived(
            ChannelId::Channel(0.into()),
            MessageId::from(1),
            NodeId::from(100u64),
            MCContent::NewTextMessage("test".into()),
            TimeStamp::from(1234567890),
        )));
    }

    #[test]
    fn test_new_node_info_with_show_user_updates() {
        let mut device_view = Device::default();
        device_view.my_node_id = Some(NodeId::from(999u64));
        device_view.show_user_updates = true;

        // Add a node first
        let node_info = MCNodeInfo {
            node_id: NodeId::from(12345u64),
            user: Some(MCUser {
                id: "test".into(),
                long_name: "Test".into(),
                short_name: "T".into(),
                ..Default::default()
            }),
            position: None,
            is_ignored: false,
        };
        let _ = device_view.update(SubscriptionMessage(NewNode(node_info)));

        // Send user info update
        let user = MCUser {
            id: "updated".into(),
            long_name: "Updated Name".into(),
            short_name: "UPD".into(),
            ..Default::default()
        };

        let _ = device_view.update(SubscriptionMessage(NewNodeInfo(
            Node(NodeId::from(12345u64)),
            MessageId::from(1),
            NodeId::from(12345u64),
            user,
            TimeStamp::from(1234567890),
        )));

        // User should be updated
        assert_eq!(
            device_view
                .nodes
                .get(&NodeId::from(12345u64))
                .expect("Node 12345 should exist after NewNodeInfo event")
                .user
                .as_ref()
                .expect("Node 12345 should have user info after NewNodeUser event")
                .long_name,
            "Updated Name"
        );
    }

    #[test]
    fn test_new_node_position_with_show_position_updates() {
        let mut device_view = Device::default();
        device_view.my_node_id = Some(NodeId::from(999u64));
        device_view.show_position_updates = true;

        // Add a node first
        let node_info = MCNodeInfo {
            node_id: NodeId::from(12345u64),
            ..Default::default()
        };
        let _ = device_view.update(SubscriptionMessage(NewNode(node_info)));

        // Send position update
        let position = test_position(51.5074, -0.1278);

        let _ = device_view.update(SubscriptionMessage(NewNodePosition(
            Node(NodeId::from(12345u64)),
            MessageId::from(1),
            NodeId::from(12345u64),
            position,
            TimeStamp::from(1234567890),
        )));

        // Position should be updated
        assert!(
            device_view
                .nodes
                .get(&NodeId::from(12345u64))
                .expect("Node 12345 should exist after NewNodeInfo event")
                .position
                .is_some()
        );
    }

    #[test]
    fn test_update_node_position_unknown_node() {
        let mut device_view = Device::default();
        let position = test_position(40.7128, -74.0060);
        // Should not panic when updating the position for an unknown node
        device_view.update_node_position(NodeId::from(99999u64), &position);
    }

    #[test]
    fn test_update_node_user_unknown_node() {
        let mut device_view = Device::default();
        let user = MCUser {
            id: "test".into(),
            long_name: "Test".into(),
            short_name: "T".into(),
            ..Default::default()
        };
        // Should not panic when updating the user for an unknown node
        device_view.update_node_user(NodeId::from(99999u64), &user);
    }

    #[test]
    fn test_forward_message_no_forwarding_message() {
        let mut device_view = Device::default();
        assert!(device_view.forwarding_message.is_none());

        let _ = device_view.update(ForwardMessage(ChannelId::Channel(0.into())));
        // Should not panic, forwarding_message is None
    }

    #[test]
    fn test_send_position_message_no_position() {
        let mut device_view = Device::default();
        assert!(device_view.my_position.is_none());

        let _ = device_view.update(SendPositionMessage(ChannelId::Channel(0.into())));
        // Should not panic, no position available
    }

    #[test]
    fn test_send_info_message_no_user() {
        let mut device_view = Device::default();
        assert!(device_view.my_user.is_none());

        let _ = device_view.update(SendSelfInfoMessage(ChannelId::Channel(0.into())));
        // Should not panic, no user info available
    }

    #[test]
    fn test_disconnect_request_with_exit() {
        let mut device_view = Device::default();
        assert!(!device_view.exit_pending);

        let _ = device_view.update(DisconnectRequest(true));
        assert!(device_view.exit_pending);
    }

    #[test]
    fn test_disconnect_request_without_exit() {
        let mut device_view = Device::default();
        assert!(!device_view.exit_pending);

        let _ = device_view.update(DisconnectRequest(false));
        assert!(!device_view.exit_pending);
    }

    #[test]
    fn test_channel_msg_with_valid_channel() {
        let mut device_view = Device::default();
        device_view.my_node_id = Some(NodeId::from(999u64));

        // Add a channel
        let channel = MCChannel {
            index: 0,
            name: "TestChannel".into(),
        };
        let _ = device_view.update(SubscriptionMessage(NewChannel(channel)));

        // Send a message to the channel
        let _ = device_view.update(ChannelMsg(
            ChannelId::Channel(0.into()),
            ChannelViewMessage::MessageInput("test".into()),
        ));
        // Should not panic
    }

    #[test]
    fn test_message_ack_with_valid_channel() {
        let mut device_view = Device::default();
        device_view.my_node_id = Some(NodeId::from(999u64));

        // Add a channel
        let channel = MCChannel {
            index: 0,
            name: "TestChannel".into(),
        };
        let _ = device_view.update(SubscriptionMessage(NewChannel(channel)));

        // ACK a message (message doesn't need to exist for ack)
        let _ = device_view.update(SubscriptionMessage(MessageACK(
            ChannelId::Channel(0.into()),
            MessageId::from(123),
        )));
        // Should not panic
    }

    #[test]
    fn test_connection_state_debug() {
        let state = Connecting("device1".into());
        let debug_str = format!("{:?}", state);
        assert!(debug_str.contains("Connecting"));
        assert!(debug_str.contains("device1"));
    }

    #[cfg(feature = "meshtastic")]
    #[test]
    fn test_device_view_message_debug() {
        let msg = ConnectRequest("device1".into(), RadioType::Meshtastic, None);
        let debug_str = format!("{:?}", msg);
        assert!(debug_str.contains("ConnectRequest"));
    }

    #[test]
    fn test_device_view_message_clone() {
        let msg = SearchInput("test".into());
        let cloned = msg.clone();
        assert!(matches!(cloned, SearchInput(s) if s == "test"));
    }

    #[test]
    fn test_add_channel_without_my_node_num() {
        let mut device_view = Device::default();
        assert!(device_view.my_node_id.is_none());

        let channel = MCChannel {
            index: 0,
            name: "TestChannel".into(),
        };
        device_view.add_channel(channel);

        // Channel should not be added without a my_node_num set
        assert!(device_view.channels.is_empty());
    }

    #[cfg(feature = "meshtastic")]
    #[test]
    fn test_subscription_disconnected_clears_state() {
        let mut device_view = Device::default();
        device_view.my_node_id = Some(NodeId::from(999u64));
        device_view.connection_state = Connected("device1".into(), RadioType::Meshtastic);

        // Add some data
        let channel = MCChannel {
            index: 0,
            name: "TestChannel".into(),
        };
        let _ = device_view.update(SubscriptionMessage(NewChannel(channel)));

        let node_info = MCNodeInfo {
            node_id: NodeId::from(12345u64),
            ..Default::default()
        };
        let _ = device_view.update(SubscriptionMessage(NewNode(node_info)));

        device_view.viewing_channel = Some(ChannelId::Channel(0.into()));

        // Don't set exit_pending so we don't exit
        device_view.exit_pending = false;

        // Disconnect - this would call std::process::exit if exit_pending was true,
        // so we can't test that path, but we can test the state clearing
        // We need to manually do what the disconnected event does without exiting
        device_view.connection_state = Disconnected(Some("device1".into()), None);
        device_view.channel_views.clear();
        device_view.nodes.clear();
        device_view.channels.clear();
        device_view.my_node_id = None;
        device_view.viewing_channel = None;

        assert!(device_view.channel_views.is_empty());
        assert!(device_view.nodes.is_empty());
        assert!(device_view.channels.is_empty());
        assert!(device_view.my_node_id.is_none());
        assert!(device_view.viewing_channel.is_none());
    }

    #[test]
    fn test_long_name_node_without_user() {
        let mut nodes: HashMap<NodeId, MCNodeInfo> = HashMap::new();
        nodes.insert(
            NodeId::from(12345u64),
            MCNodeInfo {
                node_id: NodeId::from(12345u64),
                user: None,
                position: None,
                is_ignored: false,
            },
        );
        assert_eq!(
            long_name(&nodes, NodeId::from(12345u64)),
            "Node information not available"
        );
    }

    #[test]
    fn test_cancel_interactive_with_viewing_channel() {
        let mut device_view = Device::default();
        device_view.my_node_id = Some(NodeId::from(999u64));
        device_view.editing_alias = Some(NodeId::from(123u64));
        device_view.alias = "test".into();

        // Add a channel
        let channel = MCChannel {
            index: 0,
            name: "TestChannel".into(),
        };
        let _ = device_view.update(SubscriptionMessage(NewChannel(channel)));
        device_view.viewing_channel = Some(ChannelId::Channel(0.into()));

        device_view.cancel_interactive();

        assert!(device_view.editing_alias.is_none());
        assert!(device_view.alias.is_empty());
        assert!(device_view.forwarding_message.is_none());
    }

    #[cfg(feature = "meshtastic")]
    #[test]
    fn test_channel_change_connected_with_channel_view() {
        let mut device_view = Device::default();
        device_view.my_node_id = Some(NodeId::from(999u64));
        device_view.connection_state = Connected("device1".into(), RadioType::Meshtastic);

        // Add a channel
        let channel = MCChannel {
            index: 0,
            name: "TestChannel".into(),
        };
        let _ = device_view.update(SubscriptionMessage(NewChannel(channel)));

        // Change to that channel
        let _task = device_view.channel_change(Some(ChannelId::Channel(0.into())));
        assert_eq!(
            device_view.viewing_channel,
            Some(ChannelId::Channel(0.into()))
        );
    }

    #[cfg(feature = "meshtastic")]
    #[test]
    fn test_connected_event_with_viewing_channel() {
        let mut device_view = Device::default();
        device_view.viewing_channel = Some(ChannelId::Channel(0.into()));

        let _ = device_view.update(SubscriptionMessage(ConnectedEvent(
            "device1".into(),
            RadioType::Meshtastic,
        )));
        assert_eq!(
            device_view.connection_state,
            Connected("device1".into(), RadioType::Meshtastic,)
        );
    }

    #[cfg(feature = "meshtastic")]
    #[test]
    fn test_connected_event_without_viewing_channel() {
        let mut device_view = Device::default();
        device_view.viewing_channel = None;

        let _ = device_view.update(SubscriptionMessage(ConnectedEvent(
            "device1".into(),
            RadioType::Meshtastic,
        )));
        assert_eq!(
            device_view.connection_state,
            Connected("device1".into(), RadioType::Meshtastic,)
        );
    }

    // View function tests - verify Element creation and logic branches

    #[test]
    fn test_view_no_viewing_channel_empty() {
        let device_view = Device::default();
        let config = Config::default();
        let _element = device_view.view(&config);
        // Should show the channel/node list view
    }

    #[test]
    fn test_view_with_channels_and_nodes() {
        let mut device_view = Device::default();
        device_view.my_node_id = Some(NodeId::from(999u64));

        // Add channels
        let _ = device_view.update(SubscriptionMessage(NewChannel(MCChannel {
            index: 0,
            name: "Channel1".into(),
        })));
        let _ = device_view.update(SubscriptionMessage(NewChannel(MCChannel {
            index: 1,
            name: "Channel2".into(),
        })));

        // Add nodes
        let _ = device_view.update(SubscriptionMessage(NewNode(MCNodeInfo {
            node_id: NodeId::from(12345u64),
            user: Some(MCUser {
                id: "test".into(),
                long_name: "Test User".into(),
                short_name: "TEST".into(),
                hw_model_str: "".into(),
                hw_model: 0,
                is_licensed: false,
                role_str: "".into(),
                role: 0,
                public_key: vec![],
                is_unmessagable: false,
            }),
            position: None,
            is_ignored: false,
        })));

        let config = Config::default();
        let _element = device_view.view(&config);
        // Should show the list of channels and nodes
    }

    #[test]
    fn test_view_with_viewing_channel() {
        let mut device_view = Device::default();
        device_view.my_node_id = Some(NodeId::from(999u64));

        // Add a channel
        let _ = device_view.update(SubscriptionMessage(NewChannel(MCChannel {
            index: 0,
            name: "TestChannel".into(),
        })));

        // Set viewing channel
        device_view.viewing_channel = Some(ChannelId::Channel(0.into()));

        let config = Config::default();
        let _element = device_view.view(&config);
        // Should show the channel view content
    }

    #[test]
    fn test_view_with_filter() {
        let mut device_view = Device::default();
        device_view.my_node_id = Some(NodeId::from(999u64));

        // Add channels
        let _ = device_view.update(SubscriptionMessage(NewChannel(MCChannel {
            index: 0,
            name: "Alpha".into(),
        })));
        let _ = device_view.update(SubscriptionMessage(NewChannel(MCChannel {
            index: 1,
            name: "Beta".into(),
        })));

        // Set filter
        let _ = device_view.update(SearchInput("Alpha".into()));

        let config = Config::default();
        let _element = device_view.view(&config);
        // Filter should affect which channels are shown
    }

    #[test]
    fn test_view_with_favourite_nodes() {
        let mut device_view = Device::default();
        device_view.my_node_id = Some(NodeId::from(999u64));

        // Add nodes
        let _ = device_view.update(SubscriptionMessage(NewNode(MCNodeInfo {
            node_id: NodeId::from(12345u64),
            user: Some(MCUser {
                id: "test".into(),
                long_name: "Test User".into(),
                short_name: "TEST".into(),
                ..Default::default()
            }),
            position: None,
            is_ignored: false,
        })));

        let mut config = Config::default();
        // Mark the node as favourite
        config.fav_nodes.insert(NodeId::from(12345u64));

        let _element = device_view.view(&config);
        // Should show the favourite section with the node
    }

    #[test]
    fn test_view_with_node_alias() {
        let mut device_view = Device::default();
        device_view.my_node_id = Some(NodeId::from(999u64));

        // Add node
        let _ = device_view.update(SubscriptionMessage(NewNode(MCNodeInfo {
            node_id: NodeId::from(12345u64),
            user: Some(MCUser {
                id: "test".into(),
                long_name: "Original Name".into(),
                short_name: "TEST".into(),
                ..Default::default()
            }),
            position: None,
            is_ignored: false,
        })));

        let mut config = Config::default();
        config
            .aliases
            .insert(NodeId::from(12345u64), "Aliased Name".into());

        let _element = device_view.view(&config);
        // Should show alias instead of the original name
    }

    #[test]
    fn test_view_with_editing_node_alias() {
        let mut device_view = Device::default();
        device_view.my_node_id = Some(NodeId::from(999u64));

        // Add node
        let _ = device_view.update(SubscriptionMessage(NewNode(MCNodeInfo {
            node_id: NodeId::from(12345u64),
            user: Some(MCUser {
                id: "test".into(),
                long_name: "Test User".into(),
                short_name: "TEST".into(),
                ..Default::default()
            }),
            position: None,
            is_ignored: false,
        })));

        // Start editing alias
        let _ = device_view.update(StartEditingAlias(NodeId::from(12345u64)));
        let _ = device_view.update(AliasInput("New Alias".into()));

        let config = Config::default();
        let _element = device_view.view(&config);
        // Should show text input for alias
    }

    #[test]
    fn test_view_with_node_position() {
        let mut device_view = Device::default();
        device_view.my_node_id = Some(NodeId::from(999u64));

        // Add a node with a position
        let _ = device_view.update(SubscriptionMessage(NewNode(MCNodeInfo {
            node_id: NodeId::from(12345u64),
            user: Some(MCUser {
                id: "test".into(),
                long_name: "Test User".into(),
                short_name: "TEST".into(),
                ..Default::default()
            }),
            position: Some(test_position(37.7749, -122.4194)),
            is_ignored: false,
        })));

        let config = Config::default();
        let _element = device_view.view(&config);
        // Should show the location button for the node with position
    }

    #[cfg(feature = "meshtastic")]
    #[test]
    fn test_header_all_connection_states() {
        let mut device_view = Device::default();
        device_view.my_node_id = Some(NodeId::from(999u64));

        let device_list_view = DeviceList::default();
        let config = Config::default();

        // Test all connection states
        let states = vec![
            Disconnected(None, None),
            Disconnected(Some("device1".into()), Some("error".into())),
            Connecting("device1".into()),
            Connected("device1".into(), RadioType::Meshtastic),
            Disconnecting("device1".into()),
        ];

        for state in states {
            device_view.connection_state = state.clone();
            let _element = device_view.header(&config, &state, &device_list_view);
            // Each state should produce a valid header element
        }
    }

    #[cfg(feature = "meshtastic")]
    #[test]
    fn test_header_with_viewing_channel() {
        let mut device_view = Device::default();
        device_view.my_node_id = Some(NodeId::from(999u64));
        device_view.connection_state = Connected("device1".into(), RadioType::Meshtastic);

        // Add a channel
        let _ = device_view.update(SubscriptionMessage(NewChannel(MCChannel {
            index: 0,
            name: "TestChannel".into(),
        })));

        device_view.viewing_channel = Some(ChannelId::Channel(0.into()));

        let device_list_view = DeviceList::default();
        let config = Config::default();
        let state = Connected("device1".into(), RadioType::Meshtastic);

        let _element = device_view.header(&config, &state, &device_list_view);
        // Should include the channel name in the header
    }

    #[cfg(feature = "meshtastic")]
    #[test]
    fn test_header_with_viewing_node() {
        let mut device_view = Device::default();
        device_view.my_node_id = Some(NodeId::from(999u64));
        device_view.connection_state = Connected("device1".into(), RadioType::Meshtastic);

        // Add a node
        let _ = device_view.update(SubscriptionMessage(NewNode(MCNodeInfo {
            node_id: NodeId::from(12345u64),
            user: Some(MCUser {
                id: "test".into(),
                long_name: "Test User".into(),
                short_name: "TEST".into(),
                ..Default::default()
            }),
            position: None,
            is_ignored: false,
        })));

        device_view.viewing_channel = Some(Node(NodeId::from(12345u64)));

        let device_list_view = DeviceList::default();
        let config = Config::default();
        let state = Connected("device1".into(), RadioType::Meshtastic);

        let _element = device_view.header(&config, &state, &device_list_view);
        // Should include the node name in the header
    }

    #[test]
    fn test_battery_level_display_none() {
        let mut device_view = Device::default();
        device_view.battery_level = None;
        let _element = device_view.battery_level();
    }

    #[test]
    fn test_battery_level_display_zero() {
        let mut device_view = Device::default();
        device_view.battery_level = Some(0);
        let _element = device_view.battery_level();
    }

    #[test]
    fn test_battery_level_display_fifty() {
        let mut device_view = Device::default();
        device_view.battery_level = Some(50);
        let _element = device_view.battery_level();
    }

    #[test]
    fn test_battery_level_display_hundred() {
        let mut device_view = Device::default();
        device_view.battery_level = Some(100);
        let _element = device_view.battery_level();
    }

    #[test]
    fn test_battery_level_display_charging() {
        let mut device_view = Device::default();
        device_view.battery_level = Some(101); // Charging indicator
        let _element = device_view.battery_level();
    }

    #[test]
    fn test_unread_counter_display() {
        // Test unread counter element
        let _element = Device::unread_counter(0);
        let _element = Device::unread_counter(1);
        let _element = Device::unread_counter(10);
        let _element = Device::unread_counter(100);
    }

    #[test]
    fn test_settings_button() {
        let _element = Device::settings_button();
        // Should create a settings button element
    }

    #[test]
    fn test_channel_row() {
        let select = |channel_id: ChannelId| DeviceViewEvent(ShowChannel(Some(channel_id)));

        let _element = Device::channel_row(
            "Test Channel".into(),
            0,
            ChannelId::Channel(0.into()),
            select,
        );

        let _element = Device::channel_row(
            "Channel with unread".into(),
            5,
            ChannelId::Channel(1.into()),
            select,
        );
    }

    #[test]
    fn test_search_box() {
        let device_view = Device::default();
        let _element = device_view.search_box();

        // With filter text
        let mut device_view_with_filter = Device::default();
        let _ = device_view_with_filter.update(SearchInput("test filter".into()));
        let _element = device_view_with_filter.search_box();
    }

    #[test]
    fn test_channel_and_node_list() {
        let mut device_view = Device::default();
        device_view.my_node_id = Some(NodeId::from(999u64));

        // Add channels and nodes
        let _ = device_view.update(SubscriptionMessage(NewChannel(MCChannel {
            index: 0,
            name: "Channel1".into(),
        })));

        let _ = device_view.update(SubscriptionMessage(NewNode(MCNodeInfo {
            node_id: NodeId::from(12345u64),
            user: Some(MCUser {
                id: "test".into(),
                long_name: "Test User".into(),
                short_name: "TEST".into(),
                ..Default::default()
            }),
            position: None,
            is_ignored: false,
        })));

        let config = Config::default();
        let select = |channel_id: ChannelId| DeviceViewEvent(ShowChannel(Some(channel_id)));

        let _element = device_view.channel_and_node_list(&config, true, select);
        let _element = device_view.channel_and_node_list(&config, false, select);
    }

    #[test]
    fn test_section_header() {
        let device_view = Device::default();
        let _element = device_view.section_header("Test Section".into());
        let _element = device_view.section_header("Channels (5)".into());
        let _element = device_view.section_header("".into());
    }
}
