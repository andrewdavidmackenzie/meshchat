use crate::Message::{Device, Navigation};
use crate::channel_view::{ChannelId, ChannelView, ChannelViewMessage};
use crate::channel_view_entry::ChannelViewEntry;
use crate::channel_view_entry::Payload::{
    EmojiReply, NewTextMessage, Ping, Position, TextMessageReply,
};
use crate::config::Config;
use crate::device_subscription::SubscriberMessage::{Connect, Disconnect, SendText};
use crate::device_subscription::SubscriptionEvent::{
    ConnectedEvent, ConnectionError, DeviceMeshPacket, DevicePacket, DisconnectedEvent, Ready,
};
use crate::device_subscription::{SubscriberMessage, SubscriptionEvent};
use crate::device_view::ConnectionState::{Connected, Connecting, Disconnected, Disconnecting};
use crate::device_view::DeviceViewMessage::{
    ChannelMsg, ConnectRequest, DisconnectRequest, SearchInput, SendMessage, ShowChannel,
    SubscriptionMessage,
};
use crate::styles::{
    DAY_SEPARATOR_STYLE, NO_BORDER, NO_SHADOW, VIEW_BUTTON_BORDER, button_chip_style,
    text_input_style,
};
use crate::{Message, View};
use iced::widget::button::Status::Hovered;
use iced::widget::button::{Status, Style};
use iced::widget::scrollable::Scrollbar;
use iced::widget::text::Shaping::Advanced;
use iced::widget::{
    Button, Column, Container, Row, Space, button, row, scrollable, text, text_input,
};
use iced::{Background, Center, Color, Element, Fill, Padding, Task, Theme};
use meshtastic::Message as _;
use meshtastic::protobufs::channel::Role;
use meshtastic::protobufs::channel::Role::*;
use meshtastic::protobufs::from_radio::PayloadVariant;
use meshtastic::protobufs::mesh_packet::PayloadVariant::Decoded;
use meshtastic::protobufs::telemetry::Variant::DeviceMetrics;
use meshtastic::protobufs::{Channel, FromRadio, MeshPacket, NodeInfo, PortNum};
use meshtastic::utils::stream::BleDevice;
use std::collections::HashMap;
use tokio::sync::mpsc::Sender;

const VIEW_BUTTON_HOVER_STYLE: Style = Style {
    background: Some(Background::Color(Color::from_rgba(0.0, 0.8, 0.8, 1.0))),
    text_color: Color::BLACK,
    border: VIEW_BUTTON_BORDER,
    shadow: NO_SHADOW,
};

const VIEW_BUTTON_STYLE: Style = Style {
    background: Some(Background::Color(Color::from_rgba(0.0, 1.0, 1.0, 0.0))),
    text_color: Color::WHITE,
    border: NO_BORDER,
    shadow: NO_SHADOW,
};

#[derive(Clone, PartialEq)]
pub enum ConnectionState {
    #[allow(dead_code)] // Remove this when the optional error string is used
    Disconnected(Option<BleDevice>, Option<String>),
    Connecting(BleDevice),
    Connected(BleDevice),
    Disconnecting(BleDevice),
}

#[derive(Debug, Clone)]
pub enum DeviceViewMessage {
    ConnectRequest(BleDevice, Option<ChannelId>),
    DisconnectRequest(BleDevice, bool), // bool is to exit or not
    SubscriptionMessage(SubscriptionEvent),
    ShowChannel(Option<ChannelId>),
    ChannelMsg(ChannelViewMessage),
    SendMessage(String, ChannelId),
    SearchInput(String),
}

pub struct DeviceView {
    connection_state: ConnectionState,
    subscription_sender: Option<Sender<SubscriberMessage>>, // TODO Maybe combine with Disconnected state?
    my_node_num: Option<u32>,
    pub(crate) channels: Vec<Channel>,
    // TODO elements of NodeInfo I could use
    // DeviceMetrics: battery level - in device list
    // User::long_name, User::short_name, User::hw_model
    nodes: HashMap<u32, NodeInfo>, // all nodes known to the connected radio
    pub(crate) viewing_channel: Option<ChannelId>,
    channel_views: HashMap<ChannelId, ChannelView>,
    filter: String,
    exit_pending: bool,
}

async fn request_connection(sender: Sender<SubscriberMessage>, device: BleDevice) {
    let _ = sender.send(Connect(device)).await;
}

async fn request_send(sender: Sender<SubscriberMessage>, text: String, channel_id: ChannelId) {
    let _ = sender.send(SendText(text, channel_id)).await;
}

async fn request_disconnection(sender: Sender<SubscriberMessage>) {
    let _ = sender.send(Disconnect).await;
}

async fn empty() {}

impl Default for DeviceView {
    fn default() -> Self {
        Self::new()
    }
}

impl DeviceView {
    pub fn new() -> Self {
        Self {
            connection_state: Disconnected(None, None),
            subscription_sender: None,
            channels: vec![],
            nodes: HashMap::new(),
            my_node_num: None,
            viewing_channel: None, // No channel is being shown by default
            channel_views: HashMap::new(),
            filter: String::default(),
            exit_pending: false,
        }
    }

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
                self.connection_state = Connecting(device.clone()); // TODO make state change depend on message back from subscription
                let sender = self.subscription_sender.clone();
                Task::perform(request_connection(sender.unwrap(), device.clone()), |_| {
                    Navigation(View::Device)
                })
            }
            DisconnectRequest(name, exit) => {
                self.exit_pending = exit;
                self.connection_state = Disconnecting(name); // TODO make state change depend on message back from subscription
                // Send a message to the subscription to disconnect
                let sender = self.subscription_sender.clone();
                Task::perform(request_disconnection(sender.unwrap()), |_| {
                    Navigation(View::DeviceList)
                })
            }
            ShowChannel(channel_id) => {
                if let Connected(device) = &self.connection_state {
                    let channel_id_clone = channel_id.clone();
                    let device_clone = device.clone();
                    self.viewing_channel = channel_id;
                    Task::perform(empty(), move |_| {
                        Message::SaveConfig(Config {
                            device: Some(device_clone.clone()),
                            channel_id: channel_id_clone.clone(),
                        })
                    })
                } else {
                    Task::none()
                }
            }
            SubscriptionMessage(subscription_event) => match subscription_event {
                ConnectedEvent(device) => {
                    self.connection_state = Connected(device.clone());
                    match &self.viewing_channel {
                        None => {
                            let channel_id = self.viewing_channel.clone();
                            Task::perform(empty(), move |_| {
                                Message::SaveConfig(Config {
                                    device: Some(device.clone()),
                                    channel_id: channel_id.clone(),
                                })
                            })
                        }
                        Some(channel_id) => {
                            let channel_id = channel_id.clone();
                            Task::perform(empty(), move |_| {
                                Device(ShowChannel(Some(channel_id.clone())))
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
                    Task::perform(empty(), |_| Navigation(View::DeviceList))
                }
                Ready(sender) => {
                    self.subscription_sender = Some(sender);
                    Task::none()
                }
                DevicePacket(packet) => self.handle_from_radio(packet),
                DeviceMeshPacket(packet) => self.handle_mesh_packet(&packet),
                ConnectionError(id, summary, detail) => {
                    self.connection_state = Disconnected(Some(id), Some(summary.clone()));
                    Task::perform(empty(), |_| Navigation(View::DeviceList))
                        .chain(Self::report_error(summary.clone(), detail.clone()))
                }
            },
            SendMessage(message, index) => {
                let sender = self.subscription_sender.clone();
                Task::perform(request_send(sender.unwrap(), message, index), |_| {
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

    /// Handle [FromRadio] packets coming from the radio, forwarded from the device_subscription
    fn handle_from_radio(&mut self, packet: Box<FromRadio>) -> Task<Message> {
        match packet.payload_variant {
            Some(PayloadVariant::Packet(mesh_packet)) => {
                return self.handle_mesh_packet(&mesh_packet);
            }
            Some(PayloadVariant::MyInfo(my_node_info)) => {
                self.my_node_num = Some(my_node_info.my_node_num);
            }
            Some(PayloadVariant::NodeInfo(node_info)) => self.add_user_node(node_info),
            // This Packet conveys information about a Channel that exists on the radio
            Some(PayloadVariant::Channel(channel)) => self.add_channel(channel),
            Some(PayloadVariant::QueueStatus(_)) => {
                // TODO maybe show if devices in outgoing queue?
            }
            Some(PayloadVariant::Metadata(_)) => {
                // TODO could be interesting to get device_hardware value "hw_model"
                // see https://docs.rs/meshtastic/0.1.7/meshtastic/protobufs/enum.HardwareModel.html
            }
            Some(PayloadVariant::ClientNotification(notification)) => {
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
    fn add_user_node(&mut self, node_info: NodeInfo) {
        if let Some(my_node_num) = self.my_node_num
            && my_node_num != node_info.num
            && !node_info.is_ignored
            && node_info.user.is_some()
        {
            let channel_id = ChannelId::Node(node_info.num);
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
                ChannelId::Node(mesh_packet.to)
            } else {
                // from the other node, put it in that node's channel
                ChannelId::Node(mesh_packet.from)
            }
        }
    }

    /// Handle a packet we have received from the mesh, depending on the payload variant and portnum
    fn handle_mesh_packet(&mut self, mesh_packet: &MeshPacket) -> Task<Message> {
        if let Some(Decoded(data)) = &mesh_packet.payload_variant {
            match PortNum::try_from(data.portnum) {
                Ok(PortNum::RoutingApp) => {
                    let channel_id = if mesh_packet.from == mesh_packet.to {
                        ChannelId::Channel(mesh_packet.channel as i32)
                    } else {
                        ChannelId::Node(mesh_packet.from)
                    };
                    if let Some(channel_view) = &mut self.channel_views.get_mut(&channel_id) {
                        channel_view.ack(data.request_id)
                    }
                }
                Ok(PortNum::AlertApp) => {
                    // TODO something special for an Alert message!
                    let channel_id = self.channel_id_from_packet(mesh_packet);
                    let name = self.source_name(mesh_packet);
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
                    let name = self.source_name(mesh_packet);
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
                    let position =
                        meshtastic::protobufs::Position::decode(&data.payload as &[u8]).unwrap();
                    let channel_id = self.channel_id_from_packet(mesh_packet);
                    let seen = self.viewing_channel == Some(channel_id.clone());
                    let name = self.source_name(mesh_packet);
                    if let Some(channel_view) = &mut self.channel_views.get_mut(&channel_id) {
                        let new_message = ChannelViewEntry::new(
                            Position(position.latitude_i.unwrap(), position.longitude_i.unwrap()),
                            mesh_packet.from,
                            mesh_packet.id,
                            name,
                            seen,
                        );
                        channel_view.new_message(new_message);
                    } else {
                        eprintln!("No channel for ChannelId: {}", channel_id);
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
                    // TODO could get hw_model here also for the device
                    let channel_id = self.channel_id_from_packet(mesh_packet);
                    let name = self.source_name(mesh_packet);
                    let seen = self.viewing_channel == Some(channel_id.clone());
                    if let Some(channel_view) = &mut self.channel_views.get_mut(&channel_id) {
                        let new_message = ChannelViewEntry::new(
                            Ping(user.short_name),
                            mesh_packet.from,
                            mesh_packet.id,
                            name,
                            seen,
                        );
                        channel_view.new_message(new_message);
                    } else {
                        eprintln!("No channel for ChannelId: {}", channel_id);
                    }
                }
                _ => eprintln!("Unexpected payload type from radio: {}", data.portnum),
            }
        }

        Task::none()
    }

    /// Return an Optional name to display in the message box as the source of a message.
    /// If the message is from myself, then return None.
    fn source_name(&self, mesh_packet: &MeshPacket) -> Option<String> {
        if Some(mesh_packet.from) == self.my_node_num {
            return None;
        }

        self.nodes
            .get(&mesh_packet.from)
            .map(|node_info| node_info.user.as_ref().unwrap().short_name.clone())
    }

    pub fn header<'a>(&'a self, connection_state: &'a ConnectionState) -> Element<'a, Message> {
        let mut header: Row<Message> = Row::new();

        header = match connection_state {
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
                    button = button.on_press(Device(ShowChannel(None)));
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

        match &self.viewing_channel {
            Some(ChannelId::Channel(channel_index)) => {
                let index = *channel_index as usize;
                if let Some(channel) = self.channels.get(index) {
                    // TODO do this in channel_view code like in view() below.
                    let channel_name = Self::channel_name(channel);
                    header = header
                        .push(button(text(channel_name).shaping(Advanced)).style(button_chip_style))
                }
            }
            Some(ChannelId::Node(node_id)) => {
                header = header.push(
                    button(text(self.node_name(*node_id)).shaping(Advanced))
                        .style(button_chip_style),
                )
            }
            None => {}
        }

        // Add a disconnect button on the right if we are connected
        if let Connected(device) = connection_state {
            header = header.push(Space::new(Fill, 1)).push(
                button("Disconnect")
                    .on_press(Device(DisconnectRequest(device.clone(), false)))
                    .style(button_chip_style),
            )
        }

        header.into()
    }

    pub fn view(&self) -> Element<'static, Message> {
        if let Some(channel_number) = &self.viewing_channel {
            // && let Some((_channel, packets)) = &self.channels.get(channel_number as usize)
            if let Some(channel_view) = self.channel_views.get(channel_number) {
                return channel_view.view();
            }
        }

        // If not viewing a channel/user, show the list of channels and users
        let mut channels_list = Column::new();

        if !self.channels.is_empty() {
            channels_list = channels_list.push(self.section_header("Channels"));
        }

        for (index, channel) in self.channels.iter().enumerate() {
            let channel_name = Self::channel_name(channel);

            // If there is a filter and the channel name does not contain it, don't show this row
            if !self.filter.is_empty() && !channel_name.contains(&self.filter) {
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

        if !self.nodes.is_empty() {
            channels_list = channels_list.push(self.section_header("Nodes"));
        }
        // We only store Nodes that have a valid user set
        for node_id in self.nodes.keys() {
            let node_name = self.node_name(*node_id);

            // If there is a filter and the Username does not contain it, don't show this row
            if !self.filter.is_empty() && !node_name.contains(&self.filter) {
                continue;
            }

            let channel_id = ChannelId::Node(*node_id);

            channels_list = channels_list.push(Self::node_row(
                node_name,
                self.channel_views
                    .get(&channel_id)
                    .unwrap()
                    .num_unseen_messages(),
                channel_id,
            ));
            // TODO mark as a favourite if has is_favorite set
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

    /// Add a section header between areas of the list of channels and users
    fn section_header(&self, title: &'static str) -> Element<'static, Message> {
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

    /// Return a name for the node with id node_id - prefixed with a node/device emoji
    fn node_name(&self, node_id: u32) -> String {
        let name = self
            .nodes
            .get(&node_id)
            .map(|node_info| node_info.user.as_ref().unwrap().long_name.as_ref())
            .unwrap_or("");
        format!("📱  {}", name)
    }

    fn view_button(_: &Theme, status: Status) -> Style {
        if status == Hovered {
            VIEW_BUTTON_HOVER_STYLE
        } else {
            VIEW_BUTTON_STYLE
        }
    }

    /// Create a Button that represents either a Channel or a Node
    fn channel_button(
        name: String,
        num_messages: usize,
        channel_id: ChannelId,
    ) -> Button<'static, Message> {
        button(text(format!("{} ({})", name, num_messages)).shaping(Advanced))
            .on_press(Device(ShowChannel(Some(channel_id))))
            .width(Fill)
            .style(Self::view_button)
    }

    fn node_row(
        name: String,
        num_messages: usize,
        channel_id: ChannelId,
    ) -> Button<'static, Message> {
        let row_text = format!("{} ({})", name, num_messages);
        button(text(row_text).shaping(Advanced))
            .on_press(Device(ShowChannel(Some(channel_id))))
            .width(Fill)
            .style(Self::view_button)
    }

    fn search_box(&self) -> Element<'static, Message> {
        row([text_input("Search for Channel or Node", &self.filter)
            .style(text_input_style)
            .padding([6, 6])
            .on_input(|s| Device(SearchInput(s)))
            .into()])
        .padding([0, 4]) // 6 pixels spacing minus the 2-pixel border width
        .into()
    }
}
