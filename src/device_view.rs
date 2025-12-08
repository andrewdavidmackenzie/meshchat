use crate::ConfigChangeMessage::DeviceAndChannel;
use crate::Message::{DeviceViewEvent, Navigation, ShowLocation, ToggleNodeFavourite};
use crate::View::DeviceList;
use crate::channel_view::ChannelId::Node;
use crate::channel_view::{ChannelId, ChannelView, ChannelViewMessage};
use crate::channel_view_entry::ChannelViewEntry;
use crate::channel_view_entry::Payload::{
    EmojiReply, NewTextMessage, PositionMessage, TextMessageReply, UserMessage,
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
    ChannelMsg, ConnectRequest, DisconnectRequest, SearchInput, SendInfoMessage,
    SendPositionMessage, SendTextMessage, ShowChannel, SubscriptionMessage,
};
use crate::styles::{
    DAY_SEPARATOR_STYLE, button_chip_style, channel_row_style, fav_button_style, text_input_style,
};
use crate::{Message, View, icons};
use iced::widget::scrollable::Scrollbar;
use iced::widget::text::Shaping::Advanced;
use iced::widget::{Column, Container, Row, Space, button, row, scrollable, text, text_input};
use iced::{Bottom, Center, Element, Fill, Padding, Task};
use meshtastic::Message as _;
use meshtastic::protobufs::channel::Role;
use meshtastic::protobufs::channel::Role::*;
use meshtastic::protobufs::config::device_config;
use meshtastic::protobufs::from_radio::PayloadVariant;
use meshtastic::protobufs::mesh_packet::PayloadVariant::Decoded;
use meshtastic::protobufs::telemetry::Variant::DeviceMetrics;
use meshtastic::protobufs::{Channel, FromRadio, MeshPacket, NodeInfo, PortNum, Position};
use meshtastic::utils::stream::BleDevice;
use std::collections::HashMap;
use tokio::sync::mpsc::Sender;

#[derive(Clone, PartialEq)]
pub enum ConnectionState {
    Disconnected(Option<BleDevice>, Option<String>),
    Connecting(BleDevice),
    Connected(BleDevice),
    Disconnecting(BleDevice),
}

impl Default for ConnectionState {
    fn default() -> Self {
        Disconnected(None, None)
    }
}

#[derive(Debug, Clone)]
pub enum DeviceViewMessage {
    ConnectRequest(BleDevice, Option<ChannelId>),
    DisconnectRequest(BleDevice, bool), // bool is to exit or not
    SubscriptionMessage(SubscriptionEvent),
    ShowChannel(Option<ChannelId>),
    ChannelMsg(ChannelViewMessage),
    SendTextMessage(String, ChannelId, Option<u32>), // optional reply to message id
    SendPositionMessage(ChannelId),
    SendInfoMessage(ChannelId),
    SearchInput(String),
}

#[derive(Default)]
pub struct DeviceView {
    connection_state: ConnectionState,
    subscription_sender: Option<Sender<SubscriberMessage>>,
    my_node_num: Option<u32>,
    my_position: Option<Position>,
    my_info: bool,
    pub(crate) viewing_channel: Option<ChannelId>,
    channel_views: HashMap<ChannelId, ChannelView>,
    pub(crate) channels: Vec<Channel>,
    nodes: HashMap<u32, NodeInfo>, // all nodes known to the connected radio
    filter: String,
    exit_pending: bool,
}

async fn request_connection(sender: Sender<SubscriberMessage>, device: BleDevice) {
    let _ = sender.send(Connect(device)).await;
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

    fn report_error(summary: String, detail: String) -> Task<Message> {
        Task::perform(empty(), move |_| {
            Message::AppError(summary.clone(), detail.clone())
        })
    }

    /// Return a true value to show we can show the device view, false for main to decide
    pub fn update(&mut self, device_view_message: DeviceViewMessage) -> Task<Message> {
        match device_view_message {
            ConnectRequest(device, channel_id) => {
                // save the desired channel to show for when the connection is completed later
                self.viewing_channel = channel_id;
                self.connection_state = Connecting(device.clone());
                let sender = self.subscription_sender.clone();
                Task::perform(request_connection(sender.unwrap(), device.clone()), |_| {
                    Navigation(View::Device)
                })
            }
            DisconnectRequest(name, exit) => {
                self.exit_pending = exit;
                self.connection_state = Disconnecting(name);
                // Send a message to the subscription to disconnect
                let sender = self.subscription_sender.clone();
                Task::perform(request_disconnection(sender.unwrap()), |_| {
                    Navigation(DeviceList)
                })
            }
            ShowChannel(channel_id) => {
                if let Connected(device) = &self.connection_state {
                    let channel_id_clone = channel_id.clone();
                    let device_clone = device.clone();
                    self.viewing_channel = channel_id;
                    Task::perform(empty(), move |_| {
                        Message::ConfigChange(DeviceAndChannel(
                            Some(device_clone.clone()),
                            channel_id_clone.clone(),
                        ))
                    })
                } else {
                    Task::none()
                }
            }
            SubscriptionMessage(subscription_event) => {
                self.process_subscription_event(subscription_event)
            }
            SendTextMessage(message, index, reply_to_id) => {
                let sender = self.subscription_sender.clone();
                Task::perform(
                    request_send(sender.unwrap(), message, index, reply_to_id),
                    |_| Message::None,
                )
            }
            SendPositionMessage(channel_id) => {
                if let Some(position) = &self.my_position {
                    let sender = self.subscription_sender.clone();
                    Task::perform(
                        request_send_position(sender.unwrap(), channel_id, *position),
                        |_| Message::None,
                    )
                } else {
                    Task::none()
                }
            }
            SendInfoMessage(channel_id) => {
                let sender = self.subscription_sender.clone();
                Task::perform(request_send_info(sender.unwrap(), channel_id), |_| {
                    Message::None
                })
            }
            ChannelMsg(msg) => {
                if let Some(channel_id) = &self.viewing_channel {
                    if let Some(channel_view) = self.channel_views.get_mut(channel_id) {
                        channel_view.update(msg)
                    } else {
                        Task::none()
                    }
                } else {
                    Task::none()
                }
            }
            SearchInput(filter) => {
                self.filter = filter;
                Task::none()
            }
        }
    }

    /// Process an event sent by the subscription connected to the radio
    fn process_subscription_event(
        &mut self,
        subscription_event: SubscriptionEvent,
    ) -> Task<Message> {
        match subscription_event {
            ConnectedEvent(device) => {
                self.connection_state = Connected(device.clone());
                match &self.viewing_channel {
                    None => {
                        let channel_id = self.viewing_channel.clone();
                        Task::perform(empty(), move |_| {
                            Message::ConfigChange(DeviceAndChannel(
                                Some(device.clone()),
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
                    .chain(Self::report_error(summary.clone(), detail.clone()))
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
    fn add_channel(&mut self, mut channel: Channel) {
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
                    let name = self.short_name(mesh_packet);
                    if let Some(channel_view) = &mut self.channel_views.get_mut(&channel_id) {
                        let seen = self.viewing_channel == Some(channel_id.clone());
                        let new_message = ChannelViewEntry::new(
                            NewTextMessage(String::from_utf8(data.payload.clone()).unwrap()),
                            mesh_packet.from,
                            mesh_packet.id,
                            name,
                            seen,
                        );

                        channel_view.new_message(new_message);
                    } else {
                        eprintln!("No channel for packet");
                    }
                }
                Ok(PortNum::TextMessageApp) => {
                    let channel_id = self.channel_id_from_packet(mesh_packet);
                    let name = self.short_name(mesh_packet);
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

                        let seen = self.viewing_channel == Some(channel_id.clone());
                        let new_message = ChannelViewEntry::new(
                            message,
                            mesh_packet.from,
                            mesh_packet.id,
                            name,
                            seen,
                        );

                        channel_view.new_message(new_message);
                    } else {
                        eprintln!("No channel for packet");
                    }
                }
                Ok(PortNum::PositionApp) => {
                    let position = Position::decode(&data.payload as &[u8]).unwrap();
                    let channel_id = self.channel_id_from_packet(mesh_packet);
                    let seen = self.viewing_channel == Some(channel_id.clone());
                    let name = self.short_name(mesh_packet);
                    self.update_node_position(mesh_packet.from, &position);
                    if let Some(channel_view) = &mut self.channel_views.get_mut(&channel_id) {
                        if let Some(lat) = position.latitude_i
                            && let Some(lon) = position.longitude_i
                        {
                            let new_message = ChannelViewEntry::new(
                                PositionMessage(lat, lon),
                                mesh_packet.from,
                                mesh_packet.id,
                                name,
                                seen,
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
                        println!("My Battery Level: {}", metrics.battery_level.unwrap())
                    }
                }
                Ok(PortNum::NeighborinfoApp) => println!("Neighbor Info payload"),
                Ok(PortNum::NodeinfoApp) => {
                    let user = meshtastic::protobufs::User::decode(&data.payload as &[u8]).unwrap();
                    let channel_id = self.channel_id_from_packet(mesh_packet);
                    let name = self.short_name(mesh_packet);
                    let seen = self.viewing_channel == Some(channel_id.clone());
                    if let Some(channel_view) = &mut self.channel_views.get_mut(&channel_id) {
                        let new_message = ChannelViewEntry::new(
                            UserMessage(user),
                            mesh_packet.from,
                            mesh_packet.id,
                            name,
                            seen,
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

    /// Return an Optional name to display in the message box as the source of a message.
    /// If the message is from myself, then return None.
    fn short_name(&self, mesh_packet: &MeshPacket) -> Option<String> {
        if Some(mesh_packet.from) == self.my_node_num {
            return None;
        }

        self.nodes
            .get(&mesh_packet.from)
            .map(|node_info| node_info.user.as_ref().unwrap().short_name.clone())
    }

    /// If the Node is known already, then update its Position with a PositionApp update
    /// it has sent
    fn update_node_position(&mut self, from: u32, position: &Position) {
        if let Some(node) = self.nodes.get_mut(&from) {
            node.position = Some(*position);
        }
    }

    /// Create a header view for the top of the screen depending on the current state of the app
    /// and whether we are in discovery mode or not.
    pub fn header<'a>(&'a self, state: &'a ConnectionState) -> Element<'a, Message> {
        let mut header = Row::new().padding(4).align_y(Bottom).push(
            button("Devices")
                .style(button_chip_style)
                .on_press(Navigation(DeviceList)),
        );

        header = match state {
            Disconnected(_, _) => header
                .push(Space::with_width(Fill))
                .push(button("Disconnected").style(button_chip_style)),
            Connecting(device) => {
                let name_button =
                    button(text(format!("📱 {}", device.name.as_ref().unwrap())).shaping(Advanced))
                        .style(button_chip_style);
                header = header.push(name_button);
                header
                    .push(Space::with_width(Fill))
                    .push(button("Connecting").style(button_chip_style))
            }
            Connected(device) => {
                let mut button =
                    button(text(format!("📱 {}", device.name.as_ref().unwrap())).shaping(Advanced))
                        .style(button_chip_style);
                // If viewing a channel of the device, allow navigating back to the device view
                if self.viewing_channel.is_some() {
                    button = button.on_press(DeviceViewEvent(ShowChannel(None)));
                }

                header.push(button)
            }
            Disconnecting(device) => {
                let button =
                    button(text(format!("📱 {}", device.name.as_ref().unwrap())).shaping(Advanced))
                        .style(button_chip_style);
                header = header.push(button);
                header
                    .push(Space::with_width(Fill))
                    .push(iced::widget::button("Disconnecting").style(button_chip_style))
            }
        };

        // possibly add a node/channel name button next
        match &self.viewing_channel {
            Some(ChannelId::Channel(channel_index)) => {
                let index = *channel_index as usize;
                if let Some(channel) = self.channels.get(index) {
                    let channel_name = Self::channel_name(channel);
                    header = header
                        .push(button(text(channel_name).shaping(Advanced)).style(button_chip_style))
                }
            }
            Some(Node(node_id)) => {
                if let Some(node_name) = self.node_name(*node_id) {
                    header = header
                        .push(button(text(node_name).shaping(Advanced)).style(button_chip_style))
                }
            }
            None => {}
        }

        // Add a disconnect button on the right if we are connected
        if let Connected(device) = state {
            header = header.push(Space::new(Fill, 1)).push(
                button("Disconnect")
                    .on_press(DeviceViewEvent(DisconnectRequest(device.clone(), false)))
                    .style(button_chip_style),
            )
        }

        header.into()
    }

    pub fn view(&self, config: &Config) -> Element<'_, Message> {
        if let Some(channel_number) = &self.viewing_channel
            && let Some(channel_view) = self.channel_views.get(channel_number)
        {
            return channel_view.view(self.my_position.is_some(), self.my_info);
        }

        // If not viewing a channel/user, show the list of channels and users
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
            let channel_row = Self::channel_button(
                channel_name,
                self.channel_views
                    .get(&channel_id)
                    .unwrap()
                    .num_unseen_messages(),
                channel_id,
            );
            channels_list = channels_list.push(channel_row);
        }

        // Construct list of favourite nodes
        let mut fav_nodes: Vec<(u32, String)> = vec![];

        for fav_node_id in &config.fav_nodes {
            if let Some(node_name) = self.node_name(*fav_node_id) {
                // If there is a filter and the Username does not contain it, don't show this row
                if node_name.contains(&self.filter) {
                    fav_nodes.push((*fav_node_id, node_name));
                }
            }
        }

        // If there are favourite nodes, show the header and list them
        if !fav_nodes.is_empty() {
            channels_list = channels_list
                .push(self.section_header(format!("Favourite Nodes ({})", fav_nodes.len())));
            for (fav_node_id, node_name) in fav_nodes {
                let channel_id = Node(fav_node_id);
                if let Some(channel_view) = self.channel_views.get(&channel_id) {
                    channels_list = channels_list.push(self.node_row(
                        node_name,
                        channel_view.num_unseen_messages(),
                        fav_node_id,
                        true, // Favourite
                    ));
                }
            }
        }

        let node_list = self
            .nodes
            .keys()
            .filter(|node_id| {
                !config.fav_nodes.contains(node_id) && Some(**node_id) != self.my_node_num
            })
            .collect::<Vec<_>>();
        if !node_list.is_empty() {
            channels_list =
                channels_list.push(self.section_header(format!("Nodes ({})", node_list.len())));

            for node_id in node_list {
                if let Some(node_name) = self.node_name(*node_id) {
                    if !node_name.contains(&self.filter) {
                        continue;
                    }

                    let channel_id = Node(*node_id);

                    channels_list = channels_list.push(
                        self.node_row(
                            node_name,
                            self.channel_views
                                .get(&channel_id)
                                .unwrap()
                                .num_unseen_messages(),
                            *node_id,
                            false, // Not a Favourite
                        ),
                    );
                }
            }
        }

        let channel_and_user_scroll = scrollable(channels_list)
            .direction({
                let scrollbar = Scrollbar::new().width(10.0);
                scrollable::Direction::Vertical(scrollbar)
            })
            .width(Fill)
            .height(Fill);

        Column::new()
            .push(self.search_box())
            .push(channel_and_user_scroll)
            .into()
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
            .map(|s| s.name.clone())
            .unwrap_or("".to_string());
        format!("🛜  {}", name)
    }

    /// Return the long name for the node with id node_id - prefixed with a node/device emoji -
    /// if the node is known and has a name
    fn node_name(&self, node_id: u32) -> Option<String> {
        let node = self.nodes.get(&node_id)?;
        let user = node.user.as_ref()?;
        Some(format!("📱  {}", &user.long_name))
    }

    /// Create a Button that represents either a Channel or a Node
    fn channel_button(
        name: String,
        num_messages: usize,
        channel_id: ChannelId,
    ) -> Element<'static, Message> {
        Row::new()
            .push(
                button(text(format!("{} ({})", name, num_messages)).shaping(Advanced))
                    .on_press(DeviceViewEvent(ShowChannel(Some(channel_id))))
                    .width(Fill)
                    .style(channel_row_style),
            )
            .push(Space::with_width(10))
            .into()
    }

    fn node_row(
        &self,
        name: String,
        num_messages: usize,
        node_id: u32,
        favourite: bool,
    ) -> Element<'static, Message> {
        let mut row = Row::new().push(
            button(text(format!("{} ({})", name, num_messages)).shaping(Advanced))
                .on_press(DeviceViewEvent(ShowChannel(Some(Node(node_id)))))
                .width(Fill)
                .style(channel_row_style),
        );

        row = if let Some(node) = self.nodes.get(&node_id)
            && let Some(position) = &node.position
        {
            let latitude = 0.0000001 * position.latitude_i() as f64;
            let longitude = 0.0000001 * position.longitude_i() as f64;

            row.push(
                button(text("📌").shaping(Advanced))
                    .style(fav_button_style)
                    .on_press(ShowLocation(latitude, longitude)),
            )
        } else {
            row.push(Space::with_width(30))
        };

        let icon = if favourite {
            icons::star()
        } else {
            icons::star_empty()
        };

        row.push(
            button(icon)
                .on_press(ToggleNodeFavourite(node_id))
                .style(fav_button_style),
        )
        .push(Space::with_width(10))
        .into()
    }

    fn search_box(&self) -> Element<'static, Message> {
        row([text_input("Search for Channel or Node", &self.filter)
            .style(text_input_style)
            .padding([6, 6])
            .on_input(|s| DeviceViewEvent(SearchInput(s)))
            .into()])
        .padding([0, 4]) // 6 pixels spacing minus the 2-pixel border width
        .into()
    }
}
