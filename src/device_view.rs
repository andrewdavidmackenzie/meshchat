use crate::channel_view::{ChannelId, ChannelView, ChannelViewMessage};
use crate::config::Config;
use crate::device_subscription::SubscriberMessage::{Connect, Disconnect, SendText};
use crate::device_subscription::SubscriptionEvent::{
    ConnectedEvent, ConnectionError, DevicePacket, DisconnectedEvent, MessageSent, Ready,
};
use crate::device_subscription::{SubscriberMessage, SubscriptionEvent};
use crate::device_view::ConnectionState::{Connected, Connecting, Disconnected, Disconnecting};
use crate::device_view::DeviceViewMessage::{
    ChannelMsg, ConnectRequest, DisconnectRequest, SearchInput, SendMessage, ShowChannel,
    SubscriptionMessage,
};
use crate::styles::{text_input_style, NO_BORDER, NO_SHADOW, WHITE_BORDER};
use crate::Message::Navigation;
use crate::NavigationMessage::DevicesList;
use crate::{device_subscription, name_from_id, Message, NavigationMessage};
use iced::widget::button::Status::Hovered;
use iced::widget::button::{Status, Style};
use iced::widget::scrollable::Scrollbar;
use iced::widget::{button, scrollable, text, text_input, Button, Column, Row};
use iced::{Background, Color, Element, Task, Theme};
use iced_futures::core::Length::Fill;
use iced_futures::Subscription;
use meshtastic::protobufs::channel::Role;
use meshtastic::protobufs::channel::Role::*;
use meshtastic::protobufs::from_radio::PayloadVariant;
use meshtastic::protobufs::{Channel, NodeInfo};
use meshtastic::utils::stream::BleId;
use std::collections::HashMap;
use tokio::sync::mpsc::Sender;

const VIEW_BUTTON_HOVER_STYLE: Style = Style {
    background: Some(Background::Color(Color::from_rgba(0.0, 0.8, 0.8, 1.0))),
    text_color: Color::BLACK,
    border: WHITE_BORDER,
    shadow: NO_SHADOW,
};

const VIEW_BUTTON_STYLE: Style = Style {
    background: Some(Background::Color(Color::from_rgba(0.0, 1.0, 1.0, 0.0))),
    text_color: Color::WHITE,
    border: NO_BORDER,
    shadow: NO_SHADOW,
};

#[derive(Clone)]
pub enum ConnectionState {
    #[allow(dead_code)] // Remove this when the optional error string is used
    Disconnected(Option<BleId>, Option<String>),
    Connecting(String),
    Connected(String),
    Disconnecting(String),
}

#[derive(Debug, Clone)]
pub enum DeviceViewMessage {
    ConnectRequest(String, Option<ChannelId>),
    DisconnectRequest(String),
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
    nodes: Vec<NodeInfo>, // all nodes known to the connected radio
    pub(crate) viewing_channel: Option<ChannelId>,
    channel_views: HashMap<ChannelId, ChannelView>,
    filter: String,
}

async fn request_connection(sender: Sender<SubscriberMessage>, name: String) {
    let id = BleId::from_name(&name);
    let _ = sender.send(Connect(id)).await;
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
            nodes: vec![],
            my_node_num: None,
            viewing_channel: None, // No channel is being shown by default
            channel_views: HashMap::new(),
            filter: String::default(),
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
            ConnectRequest(name, channel_id) => {
                // save the desired channel to show for when the connection is completed later
                self.viewing_channel = channel_id;
                self.connection_state = Connecting(name.clone()); // TODO make state change depend on message back from subscription
                let sender = self.subscription_sender.clone();
                Task::perform(request_connection(sender.unwrap(), name.clone()), |_| {
                    Navigation(NavigationMessage::DeviceView)
                })
            }
            DisconnectRequest(name) => {
                self.connection_state = Disconnecting(name); // TODO make state change depend on message back from subscription
                // Send a message to the subscription to disconnect
                let sender = self.subscription_sender.clone();
                Task::perform(request_disconnection(sender.unwrap()), |_| {
                    Navigation(DevicesList)
                })
            }
            ShowChannel(channel_id) => {
                if let Connected(name) = &self.connection_state {
                    let device_name = Some(name.clone());
                    let channel_id_clone = channel_id.clone();
                    self.viewing_channel = channel_id;
                    Task::perform(empty(), move |_| {
                        Message::SaveConfig(Config {
                            device_name: device_name.clone(),
                            channel_id: channel_id_clone.clone(),
                        })
                    })
                } else {
                    Task::none()
                }
            }
            SubscriptionMessage(subscription_event) => match subscription_event {
                ConnectedEvent(id) => {
                    let name = name_from_id(&id);
                    self.connection_state = Connected(name);
                    match &self.viewing_channel {
                        None => {
                            let channel_id = self.viewing_channel.clone();
                            Task::perform(empty(), move |_| {
                                Message::SaveConfig(Config {
                                    device_name: Some(name_from_id(&id)),
                                    channel_id: channel_id.clone(),
                                })
                            })
                        }
                        Some(channel_id) => {
                            let channel_id = channel_id.clone();
                            Task::perform(empty(), move |_| {
                                Message::Device(ShowChannel(Some(channel_id.clone())))
                            })
                        }
                    }
                }
                DisconnectedEvent(id) => {
                    self.connection_state = Disconnected(Some(id), None);
                    Task::perform(empty(), |_| Navigation(DevicesList))
                }
                Ready(sender) => {
                    self.subscription_sender = Some(sender);
                    Task::none()
                }
                DevicePacket(packet) => {
                    match packet.payload_variant.unwrap() {
                        PayloadVariant::Packet(mesh_packet) => {
                            // TODO determine if there is a packet for this node or user, and
                            // not a channel? Then send to node view?
                            let channel_id = ChannelId::Channel(mesh_packet.channel as i32);
                            if let Some(channel_view) = &mut self.channel_views.get_mut(&channel_id)
                            {
                                channel_view.push_packet(mesh_packet);
                            } else {
                                eprintln!(
                                    "No channel for packet received: {}",
                                    mesh_packet.channel
                                );
                            }
                        }
                        PayloadVariant::MyInfo(my_node_info) => {
                            self.my_node_num = Some(my_node_info.my_node_num);
                        }
                        PayloadVariant::NodeInfo(node_info) => {
                            if let Some(my_node_num) = self.my_node_num
                                && my_node_num != node_info.num
                            {
                                self.nodes.push(node_info)
                            }
                        }
                        // This Packet conveys information about a Channel that exists on the radio
                        PayloadVariant::Channel(mut channel) => {
                            if let Some(settings) = channel.settings.as_mut() {
                                if settings.name.is_empty() {
                                    settings.name = "Default".to_string();
                                };
                                self.channels.push(channel);
                                let channel_id =
                                    ChannelId::Channel((self.channels.len() - 1) as i32);
                                self.channel_views.insert(
                                    channel_id.clone(),
                                    ChannelView::new(channel_id, self.my_node_num.unwrap()),
                                );
                            }
                        }
                        PayloadVariant::QueueStatus(_) => {
                            // TODO maybe show if devices in outgoing queue?
                        }
                        PayloadVariant::Metadata(_) => {
                            // TODO could be interesting to get device_hardware value
                        }
                        PayloadVariant::ClientNotification(notification) => {
                            // TODO display a notification in the header
                            println!("Received notification: {}", notification.message);
                        }
                        _ => {}
                    }
                    Task::none()
                }
                MessageSent(channel_index) => {
                    if let Some(channel_view) = self.channel_views.get_mut(&channel_index) {
                        channel_view.message_sent();
                    }
                    Task::none()
                }
                ConnectionError(id, summary, detail) => {
                    self.connection_state = Disconnected(Some(id), Some(summary.clone()));
                    Task::perform(empty(), |_| Navigation(DevicesList))
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

    pub fn header<'a>(
        &'a self,
        mut header: Row<'a, Message>,
        connection_state: &'a ConnectionState,
    ) -> Row<'a, Message> {
        header = header
            .push(button("Devices").on_press(Navigation(DevicesList)))
            .push(text(" / "));

        header = match connection_state {
            Disconnected(_, _) => header.push(text("Disconnected")),
            Connecting(name) => header.push(text(format!("Connecting to {}", name))),
            Connected(name) => {
                if self.viewing_channel.is_some() {
                    header.push(button(text(name)).on_press(Message::Device(ShowChannel(None))))
                } else {
                    header.push(button(text(name)))
                }
            }
            Disconnecting(name) => header.push(text(format!("Disconnecting from {}", name))),
        };

        // TODO handle User case
        if let Some(ChannelId::Channel(channel_index)) = &self.viewing_channel {
            let index = *channel_index as usize;
            if let Some(channel) = &self.channels.get(index) {
                // TODO do this in channel_view code like in view() below.
                let channel_name = Self::channel_name(channel);
                header.push(text(" / ")).push(button(text(channel_name)))
            } else {
                header
            }
        } else {
            header
        }
    }

    pub fn view(&self) -> Element<'static, Message> {
        if let Some(channel_number) = &self.viewing_channel {
            // && let Some((_channel, packets)) = &self.channels.get(channel_number as usize)
            if let Some(channel_view) = self.channel_views.get(channel_number) {
                return channel_view.view();
            }
        }

        self.device_view()
    }

    fn device_view(&self) -> Element<'static, Message> {
        let mut channels_view = Column::new();

        for (index, channel) in self.channels.iter().enumerate() {
            // TODO show QR of the channel config
            let channel_name = Self::channel_name(channel);

            // If there is a filter and the channel name does not contain it, don't show this row
            if !self.filter.is_empty() && !channel_name.contains(&self.filter) {
                continue;
            }

            let channel_row = match Role::try_from(channel.role).unwrap() {
                Disabled => break,
                _ => {
                    let channel_id = ChannelId::Channel(index as i32);
                    Self::channel_row(
                        channel_name,
                        self.channel_views.get(&channel_id).unwrap().num_packets(),
                        channel_id,
                    )
                }
            };
            channels_view = channels_view.push(channel_row);
        }

        for node in &self.nodes {
            if !node.is_ignored
                && let Some(user) = &node.user
            {
                // If there is a filter and the Username does not contain it, don't show this row
                if !self.filter.is_empty() && !user.long_name.contains(&self.filter) {
                    continue;
                }

                channels_view = channels_view.push(Self::node_row(user.long_name.clone()));
                // TODO can add to nodes on the channel list above if channel is "populated" (not 0?)
                // TODO mark as a favourite if has is_favorite set
            }
        }

        let channel_and_user_scroll = scrollable(channels_view)
            .direction({
                let scrollbar = Scrollbar::new().width(10.0);
                scrollable::Direction::Vertical(scrollbar)
            })
            .width(Fill)
            .height(Fill);

        Column::new()
            .padding(12)
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
        num_packets: usize,
        channel_id: ChannelId,
    ) -> Button<'static, Message> {
        let row_text = format!("{} ({})", name, num_packets);
        button(text(row_text))
            .on_press(Message::Device(ShowChannel(Some(channel_id))))
            .width(Fill)
            .style(Self::view_button)
    }

    fn node_row(name: String) -> Button<'static, Message> {
        button(text(name).shaping(text::Shaping::Advanced))
            .width(Fill)
            .style(Self::view_button)
    }

    fn search_box(&self) -> Element<'static, Message> {
        // TODO move styles to constants
        text_input("Search", &self.filter)
            .style(text_input_style)
            .padding([6, 6])
            .on_input(|s| Message::Device(SearchInput(s)))
            .into()
    }

    /// Create subscriptions for events coming from a connected hardware device (radio)
    pub fn subscription(&self) -> Subscription<DeviceViewMessage> {
        Subscription::run_with_id("device", device_subscription::subscribe())
            .map(SubscriptionMessage)
    }
}
