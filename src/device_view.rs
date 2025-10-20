use crate::channel_view::{ChannelView, ChannelViewMessage};
use crate::config::Config;
use crate::device_subscription::SubscriberMessage::{Connect, Disconnect, SendText};
use crate::device_subscription::SubscriptionEvent::{
    ConnectedEvent, ConnectionError, DevicePacket, DisconnectedEvent, MessageSent, Ready,
};
use crate::device_subscription::{SubscriberMessage, SubscriptionEvent};
use crate::device_view::ConnectionState::{Connected, Connecting, Disconnected, Disconnecting};
use crate::device_view::DeviceViewMessage::{
    ChannelMsg, ConnectRequest, DisconnectRequest, SendMessage, ShowChannel, SubscriptionMessage,
};
use crate::Message::Navigation;
use crate::NavigationMessage::DevicesList;
use crate::{device_subscription, name_from_id, Message, NavigationMessage};
use iced::border::Radius;
use iced::widget::button::Status::Hovered;
use iced::widget::button::{Status, Style};
use iced::widget::scrollable::Scrollbar;
use iced::widget::{button, scrollable, text, Button, Column, Row};
use iced::{Background, Border, Color, Element, Shadow, Task, Theme};
use iced_futures::core::Length::Fill;
use iced_futures::Subscription;
use meshtastic::protobufs::channel::Role;
use meshtastic::protobufs::channel::Role::*;
use meshtastic::protobufs::from_radio::PayloadVariant;
use meshtastic::protobufs::{Channel, NodeInfo};
use meshtastic::utils::stream::BleId;
use tokio::sync::mpsc::Sender;

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
    ConnectRequest(String, Option<i32>),
    DisconnectRequest(String),
    SubscriptionMessage(SubscriptionEvent),
    ShowChannel(Option<i32>),
    ChannelMsg(ChannelViewMessage),
    SendMessage(String, i32),
}

pub struct DeviceView {
    connection_state: ConnectionState,
    subscription_sender: Option<Sender<SubscriberMessage>>, // TODO Maybe combine with Disconnected state?
    my_node_num: Option<u32>,
    pub(crate) channels: Vec<Channel>, // Upto 8 - but maybe depends on firmware
    nodes: Vec<NodeInfo>,              // all nodes known to the connected radio
    pub(crate) channel_number: Option<i32>, // Channel numbers from 0 to 7
    channel_views: Vec<ChannelView>,
}

async fn request_connection(sender: Sender<SubscriberMessage>, name: String) {
    let id = BleId::from_name(&name);
    let _ = sender.send(Connect(id)).await;
}

async fn request_send(sender: Sender<SubscriberMessage>, text: String, channel: i32) {
    let _ = sender.send(SendText(text, channel)).await;
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
            channel_number: None, // No channel is being shown by default
            channel_views: vec![],
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
            ConnectRequest(name, channel_number) => {
                // save the desired channel to show for when the connection is completed later
                self.channel_number = channel_number;
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
            ShowChannel(channel_number) => {
                if let Connected(name) = &self.connection_state {
                    let device_name = Some(name.clone());
                    self.channel_number = channel_number;
                    Task::perform(empty(), move |_| {
                        Message::SaveConfig(Config {
                            device_name: device_name.clone(),
                            channel_number,
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
                    match self.channel_number {
                        None => {
                            let channel_number = self.channel_number;
                            Task::perform(empty(), move |_| {
                                Message::SaveConfig(Config {
                                    device_name: Some(name_from_id(&id)),
                                    channel_number,
                                })
                            })
                        }
                        Some(channel_number) => Task::perform(empty(), move |_| {
                            Message::Device(ShowChannel(Some(channel_number)))
                        }),
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
                            if let Some(channel_view) =
                                &mut self.channel_views.get_mut(mesh_packet.channel as usize)
                            {
                                channel_view.push_packet(mesh_packet);
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
                        PayloadVariant::Channel(mut channel) => {
                            if let Some(settings) = channel.settings.as_mut() {
                                if settings.name.is_empty() {
                                    settings.name = "Default".to_string();
                                };
                                self.channels.push(channel);
                                self.channel_views.push(ChannelView::new(
                                    (self.channels.len() - 1) as i32,
                                    self.my_node_num.unwrap(),
                                ));
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
                    if let Some(channel_view) = self.channel_views.get_mut(channel_index as usize) {
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
                if let Some(channel_number) = self.channel_number {
                    if let Some(channel_view) = self.channel_views.get_mut(channel_number as usize)
                    {
                        channel_view.update(msg)
                    } else {
                        Task::none()
                    }
                } else {
                    Task::none()
                }
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
                if self.channel_number.is_some() {
                    header.push(button(text(name)).on_press(Message::Device(ShowChannel(None))))
                } else {
                    header.push(button(text(name)))
                }
            }
            Disconnecting(name) => header.push(text(format!("Disconnecting from {}", name))),
        };

        if let Some(channel_number) = self.channel_number {
            if let Some(channel) = &self.channels.get(channel_number as usize) {
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
        if let Some(channel_number) = self.channel_number {
            // && let Some((_channel, packets)) = &self.channels.get(channel_number as usize)
            if let Some(channel_view) = self.channel_views.get(channel_number as usize) {
                return channel_view.view();
            }
        }

        self.device_view()
    }

    fn device_view(&self) -> Element<'static, Message> {
        let mut channels_view = Column::new();
        for (index, channel) in self.channels.iter().enumerate() {
            // TODO show QR of the channel config
            let channel_row = match Role::try_from(channel.role).unwrap() {
                Disabled => break,
                _ => Self::channel_row(channel, self.channel_views[index].num_packets()),
            };
            channels_view = channels_view.push(channel_row);
        }

        for node in &self.nodes {
            if !node.is_ignored
                && let Some(user) = &node.user
            {
                let mut node_row = Row::new();
                node_row = node_row.push(
                    text(format!("User: {}", user.long_name.clone()))
                        .shaping(text::Shaping::Advanced),
                );
                channels_view = channels_view.push(node_row);
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

        let mut main_col = Column::new();
        main_col = main_col.push(channel_and_user_scroll);
        main_col.into()
    }

    fn channel_name(channel: &Channel) -> String {
        channel
            .settings
            .as_ref()
            .map(|s| s.name.clone())
            .unwrap_or("Unknown".to_string())
    }

    const RADIUS_2: Radius = Radius {
        top_left: 2.0,
        top_right: 2.0,
        bottom_right: 2.0,
        bottom_left: 2.0,
    };

    pub(crate) const NO_SHADOW: Shadow = Shadow {
        color: Color::TRANSPARENT,
        offset: iced::Vector { x: 0.0, y: 0.0 },
        blur_radius: 0.0,
    };

    pub(crate) const WHITE_BORDER: Border = Border {
        color: Color::WHITE,
        width: 2.0,
        radius: Self::RADIUS_2,
    };

    pub(crate) const NO_BORDER: Border = Border {
        color: Color::TRANSPARENT,
        width: 0.0,
        radius: Self::RADIUS_2,
    };

    const VIEW_BUTTON_HOVER_STYLE: Style = Style {
        background: Some(Background::Color(Color::from_rgba(0.0, 0.8, 0.8, 1.0))),
        text_color: Color::BLACK,
        border: Self::WHITE_BORDER,
        shadow: Self::NO_SHADOW,
    };

    const VIEW_BUTTON_STYLE: Style = Style {
        background: Some(Background::Color(Color::from_rgba(0.0, 1.0, 1.0, 0.0))),
        text_color: Color::WHITE,
        border: Self::NO_BORDER,
        shadow: Self::NO_SHADOW,
    };

    fn view_button(_: &Theme, status: Status) -> Style {
        if status == Hovered {
            Self::VIEW_BUTTON_HOVER_STYLE
        } else {
            Self::VIEW_BUTTON_STYLE
        }
    }

    fn channel_row(channel: &Channel, num_packets: usize) -> Button<'static, Message> {
        let row_text = format!("{} ({})", Self::channel_name(channel), num_packets);
        button(text(row_text))
            .on_press(Message::Device(ShowChannel(Some(channel.index))))
            .width(Fill)
            .style(Self::view_button)
    }

    /// Create subscriptions for events coming from a connected hardware device (radio)
    pub fn subscription(&self) -> Subscription<DeviceViewMessage> {
        let subscriptions = vec![
            Subscription::run_with_id("device", device_subscription::subscribe())
                .map(SubscriptionMessage),
        ];

        Subscription::batch(subscriptions)
    }
}
