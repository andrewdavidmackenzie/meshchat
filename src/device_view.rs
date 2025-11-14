use crate::channel_view::{ChannelId, ChannelView, ChannelViewMessage};
use crate::channel_view_entry::ChannelViewEntry;
use crate::channel_view_entry::Payload::{Ping, Position, TextMessage};
use crate::config::Config;
use crate::device_subscription::SubscriberMessage::{Connect, Disconnect, SendText};
use crate::device_subscription::SubscriptionEvent::{
    ConnectedEvent, ConnectionError, DeviceMeshPacket, DevicePacket, DisconnectedEvent,
    MessageSent, Ready,
};
use crate::device_subscription::{SubscriberMessage, SubscriptionEvent};
use crate::device_view::ConnectionState::{Connected, Connecting, Disconnected, Disconnecting};
use crate::device_view::DeviceViewMessage::{
    ChannelMsg, ConnectRequest, DisconnectRequest, SearchInput, SendMessage, ShowChannel,
    SubscriptionMessage,
};
use crate::styles::{chip_style, text_input_style, NO_BORDER, NO_SHADOW, VIEW_BUTTON_BORDER};
use crate::Message::{Device, Navigation};
use crate::{device_subscription, Message, View};
use iced::widget::button::Status::Hovered;
use iced::widget::button::{Status, Style};
use iced::widget::scrollable::Scrollbar;
use iced::widget::text::Shaping::Advanced;
use iced::widget::{button, row, scrollable, text, text_input, Button, Column, Row, Space};
use iced::{alignment, Background, Color, Element, Fill, Task, Theme};
use iced_futures::Subscription;
use meshtastic::protobufs::channel::Role;
use meshtastic::protobufs::channel::Role::*;
use meshtastic::protobufs::from_radio::PayloadVariant;
use meshtastic::protobufs::mesh_packet::PayloadVariant::Decoded;
use meshtastic::protobufs::{Channel, FromRadio, MeshPacket, NodeInfo, PortNum};
use meshtastic::utils::stream::BleDevice;
use meshtastic::Message as _;
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
                MessageSent(msg_test, channel_id) => {
                    if let Some(channel_view) = self.channel_views.get_mut(&channel_id) {
                        channel_view.message_sent(msg_test);
                    }
                    Task::none()
                }
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
                // TODO could be interesting to get device_hardware value
            }
            Some(PayloadVariant::ClientNotification(notification)) => {
                return Task::perform(empty(), move |_| {
                    Message::AppNotification(
                        "Radio Notification".to_string(),
                        notification.message.clone(),
                    )
                });
            }
            Some(_) => {
                println!("Unexpected payload variant: {:?}", packet.payload_variant);
            }
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

    fn handle_mesh_packet(&mut self, mesh_packet: &MeshPacket) -> Task<Message> {
        if let Some(Decoded(data)) = &mesh_packet.payload_variant {
            match PortNum::try_from(data.portnum) {
                Ok(PortNum::AlertApp) | Ok(PortNum::TextMessageApp) => {
                    let channel_id = if mesh_packet.to == u32::MAX {
                        // Destined for a channel
                        ChannelId::Channel(mesh_packet.channel as i32)
                    } else {
                        ChannelId::Node(mesh_packet.from)
                    };

                    let name = self.source_name(mesh_packet);
                    if let Some(channel_view) = &mut self.channel_views.get_mut(&channel_id) {
                        let seen = self.viewing_channel == Some(channel_id.clone());
                        let new_message = ChannelViewEntry::new(
                            TextMessage(String::from_utf8(data.payload.clone()).unwrap()),
                            mesh_packet.from,
                            name,
                            seen,
                        );

                        channel_view.new_message(new_message);
                    } else {
                        eprintln!("No channel for ChannelId: {}", channel_id);
                    }
                }
                Ok(PortNum::PositionApp) => {
                    let position =
                        meshtastic::protobufs::Position::decode(&data.payload as &[u8]).unwrap();
                    let channel_id = ChannelId::Node(mesh_packet.from);
                    let seen = self.viewing_channel == Some(channel_id.clone());
                    let name = self.source_name(mesh_packet);
                    if let Some(channel_view) = &mut self.channel_views.get_mut(&channel_id) {
                        let new_message = ChannelViewEntry::new(
                            Position(position.latitude_i.unwrap(), position.longitude_i.unwrap()),
                            mesh_packet.from,
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
                    println!("Telemetry: {telemetry:?} from {}", mesh_packet.from)
                }
                Ok(PortNum::NeighborinfoApp) => println!("Neighbor Info payload"),
                Ok(PortNum::NodeinfoApp) => {
                    let user = meshtastic::protobufs::User::decode(&data.payload as &[u8]).unwrap();
                    let channel_id = ChannelId::Node(mesh_packet.from);
                    let name = self.source_name(mesh_packet);
                    let seen = self.viewing_channel == Some(channel_id.clone());
                    if let Some(channel_view) = &mut self.channel_views.get_mut(&channel_id) {
                        let new_message = ChannelViewEntry::new(
                            Ping(user.short_name),
                            mesh_packet.from,
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
            Disconnected(_, _) => {
                println!("Disconnected");
                header.push(
                    text("Disconnected")
                        .width(Fill)
                        .align_x(alignment::Horizontal::Right),
                )
            }
            Connecting(device) => {
                let button = button(text(device.name.as_ref().unwrap())).style(chip_style);
                header = header.push(button);
                header.push(
                    text("Connecting")
                        .width(Fill)
                        .align_x(alignment::Horizontal::Right),
                )
            }
            Connected(device) => {
                let mut button = button(text(device.name.as_ref().unwrap())).style(chip_style);
                // If viewing a channel of the device, allow navigating back to the device view
                if self.viewing_channel.is_some() {
                    button = button.on_press(Device(ShowChannel(None)));
                }

                header.push(button)
            }
            Disconnecting(device) => {
                let button = button(text(device.name.as_ref().unwrap())).style(chip_style);
                header = header.push(button);

                header.push(
                    text("Disconnecting")
                        .width(Fill)
                        .align_x(alignment::Horizontal::Right),
                )
            }
        };

        match &self.viewing_channel {
            Some(ChannelId::Channel(channel_index)) => {
                let index = *channel_index as usize;
                if let Some(channel) = self.channels.get(index) {
                    // TODO do this in channel_view code like in view() below.
                    let channel_name = Self::channel_name(channel);
                    header = header.push(button(text(channel_name)).style(chip_style))
                }
            }
            Some(ChannelId::Node(node_id)) => {
                if let Some(node_info) = self.nodes.get(node_id) {
                    header = header.push(
                        button(
                            text(node_info.user.as_ref().unwrap().long_name.clone())
                                .shaping(Advanced),
                        )
                        .style(chip_style),
                    )
                }
            }
            None => {}
        }

        // Add a disconnect button on the right if we are connected
        if let Connected(device) = connection_state {
            header = header.push(Space::new(Fill, 1)).push(
                iced::widget::button("Disconnect")
                    .on_press(Device(DisconnectRequest(device.clone(), false)))
                    .style(chip_style),
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
        let mut channels_view = Column::new();

        for (index, channel) in self.channels.iter().enumerate() {
            let channel_name = Self::channel_name(channel);

            // If there is a filter and the channel name does not contain it, don't show this row
            if !self.filter.is_empty() && !channel_name.contains(&self.filter) {
                continue;
            }

            let channel_id = ChannelId::Channel(index as i32);
            let channel_row = Self::channel_row(
                channel_name,
                self.channel_views
                    .get(&channel_id)
                    .unwrap()
                    .num_unseen_messages(),
                channel_id,
            );
            channels_view = channels_view.push(channel_row);
        }

        // We only store Nodes that have a valid user set
        for (node_id, node_info) in &self.nodes {
            let user = &node_info.user.as_ref().unwrap();

            // If there is a filter and the Username does not contain it, don't show this row
            if !self.filter.is_empty() && !user.long_name.contains(&self.filter) {
                continue;
            }

            let channel_id = ChannelId::Node(*node_id);

            channels_view = channels_view.push(Self::node_row(
                user.long_name.clone(),
                self.channel_views
                    .get(&channel_id)
                    .unwrap()
                    .num_unseen_messages(),
                channel_id,
            ));
            // TODO mark as a favourite if has is_favorite set
        }

        let channel_and_user_scroll = scrollable(channels_view)
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

    fn channel_name(channel: &Channel) -> String {
        channel
            .settings
            .as_ref()
            .map(|s| s.name.clone())
            .unwrap_or("Unknown".to_string())
    }

    fn view_button(_: &Theme, status: Status) -> Style {
        if status == Hovered {
            VIEW_BUTTON_HOVER_STYLE
        } else {
            VIEW_BUTTON_STYLE
        }
    }

    fn channel_row(
        name: String,
        num_messages: usize,
        channel_id: ChannelId,
    ) -> Button<'static, Message> {
        let row_text = format!("{} ({})", name, num_messages);
        button(text(row_text))
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
        row([text_input("Search for Channel or User", &self.filter)
            .style(text_input_style)
            .padding([6, 6])
            .on_input(|s| Device(SearchInput(s)))
            .into()])
        .padding([0, 4]) // 6 pixels spacing minus the 2-pixel border width
        .into()
    }

    /// Create subscriptions for events coming from a connected hardware device (radio)
    pub fn subscription(&self) -> Subscription<DeviceViewMessage> {
        Subscription::run_with_id("device", device_subscription::subscribe())
            .map(SubscriptionMessage)
    }
}
