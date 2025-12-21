use crate::battery::{Battery, BatteryState};
use crate::channel_view::ChannelId::Node;
use crate::channel_view::{ChannelId, ChannelView, ChannelViewMessage};
use crate::channel_view_entry::ChannelViewEntry;
use crate::channel_view_entry::Payload::{
    AlertMessage, EmojiReply, NewTextMessage, PositionMessage, TextMessageReply, UserMessage,
};
use crate::config::Config;
use crate::device_subscription::SubscriberMessage::{
    Connect, Disconnect, SendInfo, SendPosition, SendText,
};
use crate::device_subscription::SubscriptionEvent::{
    ConnectedEvent, ConnectionError, DeviceMeshPacket, DevicePacket, DisconnectedEvent, Ready,
};
use crate::device_subscription::{SubscriberMessage, SubscriptionEvent};
use crate::device_view::ConnectionState::{Connected, Connecting, Disconnected, Disconnecting};
use crate::device_view::DeviceViewMessage::{
    AliasInput, ChannelMsg, ClearFilter, ConnectRequest, DisconnectRequest, ForwardMessage,
    SearchInput, SendInfoMessage, SendPositionMessage, SendTextMessage, ShowChannel,
    StartEditingAlias, StartForwardingMessage, StopForwardingMessage, SubscriptionMessage,
};

use crate::ConfigChangeMessage::DeviceAndChannel;
use crate::Message::{
    AddNodeAlias, DeviceViewEvent, Navigation, RemoveNodeAlias, ShowLocation, ToggleNodeFavourite,
};
use crate::View::DeviceList;
use crate::device_list_view::DeviceListView;
use crate::styles::{
    DAY_SEPARATOR_STYLE, button_chip_style, channel_row_style, count_style, fav_button_style,
    scrollbar_style, text_input_style, tooltip_style,
};
use crate::{Message, View, icons};
use btleplug::api::BDAddr;
use iced::widget::scrollable::Scrollbar;
use iced::widget::{
    Column, Container, Row, Space, button, container, scrollable, text, text_input, tooltip,
};
use iced::{Bottom, Center, Element, Fill, Padding, Task};
use meshtastic::Message as _;
use meshtastic::protobufs::channel::Role;
use meshtastic::protobufs::channel::Role::*;
use meshtastic::protobufs::config::device_config;
use meshtastic::protobufs::from_radio::PayloadVariant;
use meshtastic::protobufs::mesh_packet::PayloadVariant::Decoded;
use meshtastic::protobufs::telemetry::Variant::DeviceMetrics;
use meshtastic::protobufs::{Channel, FromRadio, MeshPacket, NodeInfo, PortNum, Position, User};
use std::collections::HashMap;
use tokio::sync::mpsc::Sender;

#[derive(Clone, PartialEq)]
pub enum ConnectionState {
    Disconnected(Option<BDAddr>, Option<String>),
    Connecting(BDAddr),
    Connected(BDAddr),
    Disconnecting(BDAddr),
}

impl Default for ConnectionState {
    fn default() -> Self {
        Disconnected(None, None)
    }
}

#[derive(Debug, Clone)]
pub enum DeviceViewMessage {
    ConnectRequest(BDAddr, Option<ChannelId>),
    DisconnectRequest(BDAddr, bool), // bool is to exit or not
    SubscriptionMessage(SubscriptionEvent),
    ShowChannel(Option<ChannelId>),
    ChannelMsg(ChannelViewMessage),
    SendTextMessage(String, ChannelId, Option<u32>), // optional reply to message id
    SendPositionMessage(ChannelId),
    SendInfoMessage(ChannelId),
    SearchInput(String),
    StartEditingAlias(u32),
    AliasInput(String),
    StartForwardingMessage(ChannelViewEntry),
    ForwardMessage(ChannelId),
    StopForwardingMessage,
    ClearFilter,
}

#[derive(Default)]
pub struct DeviceView {
    connection_state: ConnectionState,
    subscription_sender: Option<Sender<SubscriberMessage>>,
    my_node_num: Option<u32>,
    my_position: Option<Position>,
    my_info: bool,
    viewing_channel: Option<ChannelId>,
    /// Map of ChannelViews, indexed by ChannelId
    pub channel_views: HashMap<ChannelId, ChannelView>,
    channels: Vec<Channel>,
    nodes: HashMap<u32, NodeInfo>, // all nodes known to the connected radio
    filter: String,
    exit_pending: bool,
    battery_level: Option<u32>,
    editing_alias: Option<u32>,
    alias: String,
    pub forwarding_message: Option<ChannelViewEntry>,
}

async fn request_connection(sender: Sender<SubscriberMessage>, mac_address: BDAddr) {
    let _ = sender.send(Connect(mac_address)).await;
}

async fn request_send(
    sender: Sender<SubscriberMessage>,
    text: String,
    channel_id: ChannelId,
    reply_to_id: Option<u32>,
) {
    let _ = sender.send(SendText(text, channel_id, reply_to_id)).await;
}

async fn request_send_position(
    sender: Sender<SubscriberMessage>,
    channel_id: ChannelId,
    position: Position,
) {
    let _ = sender.send(SendPosition(channel_id, position)).await;
}

async fn request_send_info(sender: Sender<SubscriberMessage>, channel_id: ChannelId) {
    let _ = sender.send(SendInfo(channel_id)).await;
}

async fn request_disconnection(sender: Sender<SubscriberMessage>) {
    let _ = sender.send(Disconnect).await;
}

async fn empty() {}

impl DeviceView {
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

    /// Return a true value to show we can show the device view, false for main to decide
    pub fn update(&mut self, device_view_message: DeviceViewMessage) -> Task<Message> {
        match device_view_message {
            ConnectRequest(mac_address, channel_id) => {
                // save the desired channel to show for when the connection is completed later
                self.connection_state = Connecting(mac_address);
                let sender = self.subscription_sender.clone();
                return Task::perform(request_connection(sender.unwrap(), mac_address), |_| {
                    Navigation(View::Device(channel_id))
                });
            }
            DisconnectRequest(mac_address, exit) => {
                self.exit_pending = exit;
                self.connection_state = Disconnecting(mac_address);
                // Send a message to the subscription to disconnect
                let sender = self.subscription_sender.clone();
                return Task::perform(request_disconnection(sender.unwrap()), |_| {
                    Navigation(DeviceList)
                });
            }
            ShowChannel(channel_id) => {
                return self.channel_change(channel_id.clone());
            }
            SubscriptionMessage(subscription_event) => {
                return self.process_subscription_event(subscription_event);
            }
            SendTextMessage(message, index, reply_to_id) => {
                if let Some(sender) = self.subscription_sender.clone() {
                    return Task::perform(
                        request_send(sender, message, index, reply_to_id),
                        |_| Message::None,
                    );
                }
            }
            SendPositionMessage(channel_id) => {
                if let Some(position) = &self.my_position
                    && let Some(sender) = self.subscription_sender.clone()
                {
                    return Task::perform(
                        request_send_position(sender, channel_id, *position),
                        |_| Message::None,
                    );
                }
            }
            SendInfoMessage(channel_id) => {
                let sender = self.subscription_sender.clone();
                return Task::perform(request_send_info(sender.unwrap(), channel_id), |_| {
                    Message::None
                });
            }
            ChannelMsg(msg) => {
                if let Some(channel_id) = &self.viewing_channel
                    && let Some(channel_view) = self.channel_views.get_mut(channel_id)
                {
                    return channel_view.update(msg);
                }
            }
            SearchInput(filter) => self.filter = filter,
            AliasInput(alias) => self.alias = alias,
            StartEditingAlias(node_id) => self.start_editing_alias(node_id),
            StartForwardingMessage(channel_view_entry) => {
                self.forwarding_message = Some(channel_view_entry)
            }
            StopForwardingMessage => self.forwarding_message = None,
            ForwardMessage(channel_id) => {
                let entry = self.forwarding_message.take();
                return self.forward_message(channel_id, entry);
            }
            ClearFilter => self.filter.clear(),
        }

        Task::none()
    }

    /// ave the new channel (which could be None) and if we are connected to a device
    /// asynchronously, save the modified config
    fn channel_change(&mut self, channel_id: Option<ChannelId>) -> Task<Message> {
        if self.viewing_channel != channel_id {
            self.viewing_channel = channel_id.clone();

            if let Some(channel) = &channel_id
                && let Connected(mac_address) = &self.connection_state
                && self.channel_views.contains_key(channel)
            {
                let channel_id = channel_id.clone();
                let mac_address = *mac_address;
                return Task::perform(empty(), move |_| {
                    Message::ConfigChange(DeviceAndChannel(Some(mac_address), channel_id))
                });
            }
        }
        Task::none()
    }

    /// Forward a message by sending to a channel - with the prefix "FWD:"
    fn forward_message(
        &mut self,
        channel_id: ChannelId,
        entry: Option<ChannelViewEntry>,
    ) -> Task<Message> {
        if let Some(channel_view_entry) = entry
            && let Some(sender) = self.subscription_sender.clone()
        {
            let message_text = format!(
                "FWD from '{}': {}\n",
                short_name(&self.nodes, channel_view_entry.from()),
                channel_view_entry.payload()
            );
            Task::perform(
                request_send(sender, message_text, channel_id.clone(), None),
                |_| DeviceViewEvent(ShowChannel(Some(channel_id))),
            )
        } else {
            Task::none()
        }
    }

    /// Called when the user selects to alias a node name
    fn start_editing_alias(&mut self, node_id: u32) {
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
            ConnectedEvent(mac_address) => {
                self.connection_state = Connected(mac_address);
                match &self.viewing_channel {
                    None => {
                        let channel_id = self.viewing_channel.clone();
                        Task::perform(empty(), move |_| {
                            Message::ConfigChange(DeviceAndChannel(
                                Some(mac_address),
                                channel_id.clone(),
                            ))
                        })
                    }
                    Some(channel_id) => {
                        let channel_id = channel_id.clone();
                        Task::perform(empty(), move |_| {
                            DeviceViewEvent(ShowChannel(Some(channel_id.clone())))
                        })
                    }
                }
            }
            DisconnectedEvent(id) => {
                if self.exit_pending {
                    std::process::exit(0);
                }
                self.connection_state = Disconnected(Some(id), None);
                self.channel_views.clear();
                self.nodes.clear();
                self.channels.clear();
                self.my_node_num = None;
                self.viewing_channel = None;
                Task::perform(empty(), |_| Navigation(DeviceList))
            }
            Ready(sender) => {
                self.subscription_sender = Some(sender);
                Task::none()
            }
            DevicePacket(packet) => self.handle_from_radio(packet),
            DeviceMeshPacket(packet) => self.handle_mesh_packet(&packet),
            ConnectionError(id, summary, detail) => {
                self.connection_state = Disconnected(Some(id), Some(summary.clone()));
                Task::perform(empty(), |_| Navigation(DeviceList))
                    .chain(Task::perform(empty(), move |_| {
                        Message::AppError(summary.clone(), detail.clone())
                    }))
            }
        }
    }

    /// Handle [FromRadio] packets coming from the radio, forwarded from the device_subscription
    fn handle_from_radio(&mut self, packet: Box<FromRadio>) -> Task<Message> {
        match packet.payload_variant {
            Some(PayloadVariant::Packet(mesh_packet)) => {
                return self.handle_mesh_packet(&mesh_packet);
            }
            Some(PayloadVariant::MyInfo(my_node_info)) => {
                self.my_node_num = Some(my_node_info.my_node_num);
            }
            // Information about a Node that exists on the radio - which could be myself
            Some(PayloadVariant::NodeInfo(node_info)) => self.add_node(node_info),
            // This Packet conveys information about a Channel that exists on the radio
            Some(PayloadVariant::Channel(channel)) => self.add_channel(channel),
            Some(PayloadVariant::ClientNotification(notification)) => {
                // A notification message from the device to the client To be used for important
                // messages that should to be displayed to the user in the form of push
                // notifications or validation messages when saving invalid configuration.
                return Task::perform(empty(), move |_| {
                    Message::AppNotification(
                        "Radio Notification".to_string(),
                        notification.message.clone(),
                    )
                });
            }
            Some(_) => {}
            _ => println!("Error parsing packet: {:?}", packet.payload_variant),
        }

        Task::none()
    }

    // Add a new node to the list if it has the User info we want and is not marked to be ignored
    fn add_node(&mut self, node_info: NodeInfo) {
        if Some(node_info.num) == self.my_node_num {
            self.my_position = node_info.position;
            self.my_info = true;
        }

        if !node_info.is_ignored
            && node_info.user.as_ref().map(|u| u.role()) == Some(device_config::Role::Client)
        {
            let channel_id = Node(node_info.num);
            self.nodes.insert(node_info.num, node_info);
            self.channel_views.insert(
                channel_id.clone(),
                ChannelView::new(channel_id, self.my_node_num.unwrap()),
            );
        }
    }

    // Add a channel from the radio to the list if it is not disabled and has some Settings
    pub fn add_channel(&mut self, mut channel: Channel) {
        if !matches!(Role::try_from(channel.role).unwrap(), Disabled)
            && let Some(settings) = channel.settings.as_mut()
        {
            if settings.name.is_empty() {
                settings.name = "Default".to_string();
            };
            self.channels.push(channel);
            let channel_id = ChannelId::Channel((self.channels.len() - 1) as i32);
            self.channel_views.insert(
                channel_id.clone(),
                ChannelView::new(channel_id, self.my_node_num.unwrap()),
            );
        }
    }

    /// Figure out which channel we should show a message in a [MeshPacket]
    /// i.e., is a broadcast message in a channel, or a DM to/from my node.
    fn channel_id_from_packet(&mut self, mesh_packet: &MeshPacket) -> ChannelId {
        if mesh_packet.to == u32::MAX {
            // Destined for a channel
            ChannelId::Channel(mesh_packet.channel as i32)
        } else {
            // Destined for a Node
            if Some(mesh_packet.from) == self.my_node_num {
                // from me to a node - put it in that node's channel
                Node(mesh_packet.to)
            } else {
                // from the other node, put it in that node's channel
                Node(mesh_packet.from)
            }
        }
    }

    /// Handle a packet we have received from the mesh, depending on the payload variant and portnum
    fn handle_mesh_packet(&mut self, mesh_packet: &MeshPacket) -> Task<Message> {
        if let Some(Decoded(data)) = &mesh_packet.payload_variant {
            match PortNum::try_from(data.portnum) {
                Ok(PortNum::RoutingApp) => {
                    // An ACK
                    let channel_id = if mesh_packet.from == mesh_packet.to {
                        // To a channel broadcast message
                        ChannelId::Channel(mesh_packet.channel as i32)
                    } else {
                        // To a DM to a Node
                        Node(mesh_packet.from)
                    };
                    if let Some(channel_view) = &mut self.channel_views.get_mut(&channel_id) {
                        channel_view.ack(data.request_id)
                    }
                }
                Ok(PortNum::AlertApp) => {
                    let channel_id = self.channel_id_from_packet(mesh_packet);
                    if let Some(channel_view) = &mut self.channel_views.get_mut(&channel_id) {
                        let new_message = ChannelViewEntry::new(
                            AlertMessage(String::from_utf8(data.payload.clone()).unwrap()),
                            mesh_packet.from,
                            mesh_packet.id,
                        );

                        channel_view.new_message(new_message);
                    } else {
                        eprintln!("No channel for packet");
                    }
                }
                Ok(PortNum::TextMessageApp) => {
                    let channel_id = self.channel_id_from_packet(mesh_packet);
                    if let Some(channel_view) = &mut self.channel_views.get_mut(&channel_id) {
                        let message = if data.reply_id == 0 {
                            NewTextMessage(String::from_utf8(data.payload.clone()).unwrap())
                        } else {
                            // Emoji reply to an earlier message
                            if data.emoji == 1 {
                                EmojiReply(
                                    data.reply_id,
                                    String::from_utf8(data.payload.clone()).unwrap(),
                                )
                            } else {
                                // Text reply to an earlier message
                                TextMessageReply(
                                    data.reply_id,
                                    String::from_utf8(data.payload.clone()).unwrap(),
                                )
                            }
                        };

                        let new_message =
                            ChannelViewEntry::new(message, mesh_packet.from, mesh_packet.id);

                        channel_view.new_message(new_message);
                    } else {
                        eprintln!("No channel for packet");
                    }
                }
                Ok(PortNum::PositionApp) => {
                    let position = Position::decode(&data.payload as &[u8]).unwrap();
                    let channel_id = self.channel_id_from_packet(mesh_packet);
                    self.update_node_position(mesh_packet.from, &position);
                    if let Some(channel_view) = &mut self.channel_views.get_mut(&channel_id) {
                        if let Some(lat) = position.latitude_i
                            && let Some(lon) = position.longitude_i
                        {
                            let new_message = ChannelViewEntry::new(
                                PositionMessage(lat, lon),
                                mesh_packet.from,
                                mesh_packet.id,
                            );
                            channel_view.new_message(new_message);
                        } else {
                            eprintln!("No lat/lon for Position: {:?}", position);
                        }
                    } else {
                        eprintln!("No channel for: {}", channel_id);
                    }
                }
                Ok(PortNum::TelemetryApp) => {
                    let telemetry =
                        meshtastic::protobufs::Telemetry::decode(&data.payload as &[u8]).unwrap();
                    if mesh_packet.from == self.my_node_num.unwrap()
                        && let Some(DeviceMetrics(metrics)) = telemetry.variant
                    {
                        self.battery_level = metrics.battery_level;
                    }
                }
                Ok(PortNum::NeighborinfoApp) => println!("Neighbor Info payload"),
                Ok(PortNum::NodeinfoApp) => {
                    let user = User::decode(&data.payload as &[u8]).unwrap();
                    let channel_id = self.channel_id_from_packet(mesh_packet);
                    if let Some(channel_view) = &mut self.channel_views.get_mut(&channel_id) {
                        let new_message = ChannelViewEntry::new(
                            UserMessage(user),
                            mesh_packet.from,
                            mesh_packet.id,
                        );
                        channel_view.new_message(new_message);
                    } else {
                        eprintln!("NodeInfoApp: No channel for: {}", user.long_name);
                    }
                }

                _ => eprintln!("Unexpected payload type from radio: {}", data.portnum),
            }
        }

        Task::none()
    }

    /// If the Node is known already, then update its Position with a PositionApp update
    /// it has sent
    /// NOTE: This position maybe more recent, but it could have less accuracy, as position
    /// accuracy is set per channel. Consider deciding whether to update or not if it has lower
    /// accuracy depending on the age of the previous position?
    fn update_node_position(&mut self, from: u32, position: &Position) {
        if let Some(node) = self.nodes.get_mut(&from) {
            node.position = Some(*position);
        }
    }

    /// Create a header view for the top of the screen depending on the current state of the app
    /// and whether we are in discovery mode or not.
    pub fn header<'a>(
        &'a self,
        config: &'a Config,
        state: &'a ConnectionState,
        device_list_view: &DeviceListView,
    ) -> Element<'a, Message> {
        let mut header = Row::new().padding(4).align_y(Center).push(
            button("Devices")
                .style(button_chip_style)
                .on_press(Navigation(DeviceList)),
        );

        header = match state {
            Disconnected(_, _) => header
                .push(Space::new().width(Fill))
                .push(button("Disconnected").style(button_chip_style)),
            Connecting(mac_address) => {
                let name_button = button(text(format!(
                    "📱 {}",
                    device_list_view.device_name_or_alias(mac_address, config)
                )))
                .style(button_chip_style);
                header = header.push(name_button);
                header
                    .push(Space::new().width(Fill))
                    .push(button("Connecting").style(button_chip_style))
            }
            Connected(device) => {
                let name_row = Row::new()
                    .push(text(format!(
                        "📱 {}",
                        device_list_view.device_name_or_alias(device, config)
                    )))
                    .push(Space::new().width(4))
                    .push(Self::unread_counter(self.unread_count()));
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
            Disconnecting(device) => {
                let button = button(text(format!(
                    "📱 {}",
                    device_list_view.device_name_or_alias(device, config)
                )))
                .style(button_chip_style);
                header = header.push(button);
                header
                    .push(Space::new().width(Fill))
                    .push(iced::widget::button("Disconnecting").style(button_chip_style))
            }
        };

        // possibly add a node/channel name button next
        match &self.viewing_channel {
            Some(ChannelId::Channel(channel_index)) => {
                let index = *channel_index as usize;
                if let Some(channel) = self.channels.get(index) {
                    let channel_name = Self::channel_name(channel);
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
        if let Connected(mac_address) = state {
            header = header.push(Space::new().width(Fill)).push(
                button("Disconnect")
                    .on_press(DeviceViewEvent(DisconnectRequest(*mac_address, false)))
                    .style(button_chip_style),
            )
        }

        header.into()
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
            Battery::new().state(battery_state),
            text(tooltip_text),
            tooltip::Position::Bottom,
        )
        .style(tooltip_style)
        .into()
    }

    /// Count all the unread messages available to this device across channels and nodes
    pub fn unread_count(&self) -> usize {
        self.channel_views
            .values()
            .fold(0, |acc, channel| acc + channel.unread_count())
    }

    /// Create the Element that shows the channels, nodes, etc.
    pub fn view<'a>(&'a self, config: &'a Config) -> Element<'a, Message> {
        if let Some(channel_number) = &self.viewing_channel
            && let Some(channel_view) = self.channel_views.get(channel_number)
        {
            return channel_view.view(
                &self.nodes,
                self.my_position.is_some(),
                self.my_info,
                self,
                config,
            );
        }

        let select = |channel_number: ChannelId| DeviceViewEvent(ShowChannel(Some(channel_number)));

        // If not viewing a channel/user, show the list of channels and users
        let channel_and_node_scroll = self.channel_and_node_list(config, true, select);

        // Add a search box at the top, outside the scrollable area
        Column::new()
            .push(self.search_box())
            .push(channel_and_node_scroll)
            .into()
    }

    /// Create a list of channels and nodes in this device with a button to select one of them
    pub fn channel_and_node_list<'a>(
        &'a self,
        config: &'a Config,
        add_buttons: bool,
        select: fn(ChannelId) -> Message,
    ) -> Element<'a, Message> {
        // If not viewing a channel/user, show the list of channels and users
        let mut channels_list = self.channel_list(add_buttons, select);

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
    fn channel_list(
        &self,
        add_buttons: bool,
        select: fn(ChannelId) -> Message,
    ) -> Column<'_, Message> {
        let mut channels_list = Column::new();

        if !self.channels.is_empty() {
            channels_list = channels_list
                .push(self.section_header(format!("Channels ({})", self.channels.len())));
        }

        for (index, channel) in self.channels.iter().enumerate() {
            let channel_name = Self::channel_name(channel);

            // If there is a filter and the channel name does not contain it, don't show this row
            if !channel_name.contains(&self.filter) {
                continue;
            }

            let channel_id = ChannelId::Channel(index as i32);
            let channel_row = Self::channel_row(
                channel_name,
                self.channel_views.get(&channel_id).unwrap().unread_count(),
                channel_id,
                add_buttons,
                select,
            );
            channels_list = channels_list.push(channel_row);
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
        let mut fav_nodes: Vec<u32> = vec![];

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
        fav_nodes.retain(|fav_node_id| Some(*fav_node_id) != self.my_node_num);

        // If there are favourite nodes, show the header and list them
        if !fav_nodes.is_empty() {
            channels_list = channels_list
                .push(self.section_header(format!("Favourite Nodes ({})", fav_nodes.len())));
            for fav_node_id in fav_nodes {
                let channel_id = Node(fav_node_id);
                if let Some(channel_view) = self.channel_views.get(&channel_id) {
                    channels_list = channels_list.push(self.node_row(
                        channel_view.unread_count(),
                        fav_node_id,
                        true, // Favourite
                        config,
                        add_buttons,
                        select,
                    ));
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
                !config.fav_nodes.contains(node_id) && Some(**node_id) != self.my_node_num
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

                channels_list = channels_list.push(self.node_row(
                    self.channel_views.get(&channel_id).unwrap().unread_count(),
                    *node_id,
                    false, // Not a Favourite
                    config,
                    add_buttons,
                    select,
                ));
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

    /// Return a name for the channel - prefixed with a node/device emoji
    fn channel_name(channel: &Channel) -> String {
        let name = channel
            .settings
            .as_ref()
            .map(|s| s.name.as_str())
            .unwrap_or("");
        format!("🛜  {}", name)
    }

    /// Return the long name for the node with id node_id - prefixed with a node/device emoji -
    /// if the node is known and has a name
    fn aliased_long_name<'a>(&'a self, config: &'a Config, node_id: u32) -> Option<&'a str> {
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
        num_messages: usize,
        channel_id: ChannelId,
        _add_buttons: bool,
        select: fn(ChannelId) -> Message,
    ) -> Element<'static, Message> {
        let name_row = Row::new()
            .push(text(name))
            .push(Space::new().width(4))
            .push(Self::unread_counter(num_messages));

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
        node_id: u32,
        favourite: bool,
        config: &'a Config,
        add_buttons: bool,
        select: fn(ChannelId) -> Message,
    ) -> Element<'a, Message> {
        let user_name: &str = self
            .nodes
            .get(&node_id)
            .and_then(|node_info| node_info.user.as_ref().map(|user| user.long_name.as_ref()))
            .unwrap();

        let name_element: Element<'a, Message> = if let Some(alias) = config.aliases.get(&node_id) {
            tooltip(
                text(alias),
                text(format!("Original user name: {}", user_name)),
                tooltip::Position::Right,
            )
            .style(tooltip_style)
            .into()
        } else if let Some(editing_node_id) = self.editing_alias
            && editing_node_id == node_id
        {
            text_input("Enter alias for this user name", &self.alias)
                .on_input(|s| DeviceViewEvent(AliasInput(s)))
                .on_submit(AddNodeAlias(editing_node_id, self.alias.clone()))
                .style(text_input_style)
                .into()
        } else {
            text(user_name.to_string()).into()
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
        node_id: u32,
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

        // Add a button to show the location of the node if it has one
        node_row = if let Some(node) = self.nodes.get(&node_id)
            && let Some(position) = &node.position
        {
            node_row.push(
                tooltip(
                    button(text("📌"))
                        .style(fav_button_style)
                        .on_press(ShowLocation(position.latitude_i(), position.longitude_i())),
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
        let mut clear_button = button(text("⨂").size(18))
            .style(button_chip_style)
            .padding(Padding::from([6, 6]));
        if !self.filter.is_empty() {
            clear_button = clear_button.on_press(DeviceViewEvent(ClearFilter));
        }

        Row::new()
            .push(
                text_input("Search for Channel or Node", &self.filter)
                    .style(text_input_style)
                    .padding([6, 6])
                    .on_input(|s| DeviceViewEvent(SearchInput(s))),
            )
            .push(Space::new().width(4.0))
            .push(clear_button)
            .push(Space::new().width(4.0))
            .padding([0, 4])
            .align_y(Center)
            .into()
    }
}

/// Return a name to display in the message box as the source of a message.
/// If the message is from myself, then return None.
pub fn short_name(nodes: &HashMap<u32, NodeInfo>, from: u32) -> &str {
    nodes
        .get(&from)
        .and_then(|node_info: &NodeInfo| node_info.user.as_ref())
        .map(|user: &User| user.short_name.as_ref())
        .unwrap_or("????")
}
