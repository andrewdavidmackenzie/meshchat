use crate::channel_view::channel_view;
use crate::config::Config;
use crate::device_subscription::SubscriberMessage::{Connect, Disconnect, SendText};
use crate::device_subscription::SubscriptionEvent::{
    ConnectedEvent, ConnectionError, DevicePacket, DisconnectedEvent, MessageSent, Ready,
};
use crate::device_subscription::{SubscriberMessage, SubscriptionEvent};
use crate::device_view::ConnectionState::{Connected, Connecting, Disconnected, Disconnecting};
use crate::device_view::DeviceViewMessage::{
    ConnectRequest, DisconnectRequest, MessageInput, SendMessage, ShowChannel, SubscriptionMessage,
};
use crate::Message::Navigation;
use crate::{device_subscription, name_from_id, Message, NavigationMessage};
use iced::widget::scrollable::Scrollbar;
use iced::widget::{button, scrollable, text, Column, Row};
use iced::{Element, Task};
use iced_futures::core::Length::Fill;
use iced_futures::Subscription;
use meshtastic::protobufs::channel::Role;
use meshtastic::protobufs::channel::Role::*;
use meshtastic::protobufs::from_radio::PayloadVariant;
use meshtastic::protobufs::{Channel, MeshPacket, NodeInfo};
use meshtastic::utils::stream::BleId;
use tokio::sync::mpsc::Sender;

#[derive(Clone)]
pub enum ConnectionState {
    Disconnected,
    Connecting(String),
    Connected(String),
    Disconnecting(String),
}

#[derive(Debug, Clone)]
pub enum DeviceViewMessage {
    ConnectRequest(String, Option<i32>),
    DisconnectRequest(String),
    SubscriptionMessage(SubscriptionEvent),
    ShowChannel(i32),
    MessageInput(String),
    SendMessage,
}

pub struct DeviceView {
    connection_state: ConnectionState,
    subscription_sender: Option<Sender<SubscriberMessage>>, // TODO Maybe combine with Disconnected state?
    my_node_num: Option<u32>,
    pub(crate) channels: Vec<(Channel, Vec<MeshPacket>)>, // Upto 8 - but maybe depends on firmware
    nodes: Vec<NodeInfo>,                                 // all nodes known to the connected radio
    pub(crate) channel_number: Option<i32>,               // Channel numbers from 0 to 7
    pub message: String,                                  // Message typed in so far
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

impl DeviceView {
    pub fn new() -> Self {
        Self {
            connection_state: Disconnected,
            subscription_sender: None,
            channels: vec![],
            nodes: vec![],
            my_node_num: None,
            channel_number: None, // No channel is being shown by default
            message: String::new(),
        }
    }

    pub fn connection_state(&self) -> &ConnectionState {
        &self.connection_state
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
                    Navigation(NavigationMessage::DevicesList)
                })
            }
            ShowChannel(channel_number) => {
                if let Connected(name) = &self.connection_state {
                    let device_name = Some(name.clone());
                    self.channel_number = Some(channel_number);
                    Task::perform(empty(), move |_| {
                        Message::SaveConfig(Config {
                            device_name: device_name.clone(),
                            channel_number: Some(channel_number),
                        })
                    })
                } else {
                    Task::none()
                }
            }
            // TODO move some of this to channel view?
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
                            Message::Device(ShowChannel(channel_number))
                        }),
                    }
                }
                DisconnectedEvent(_) => {
                    self.connection_state = Disconnected;
                    Task::perform(empty(), |_| Navigation(NavigationMessage::DevicesList))
                }
                Ready(sender) => {
                    self.subscription_sender = Some(sender);
                    Task::none()
                }
                DevicePacket(packet) => {
                    match packet.payload_variant.unwrap() {
                        PayloadVariant::Packet(mesh_packet) => {
                            if let Some((_, packets)) =
                                &mut self.channels.get_mut(mesh_packet.channel as usize)
                            {
                                packets.push(mesh_packet);
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
                        PayloadVariant::Config(_) => {}
                        PayloadVariant::LogRecord(_) => {}
                        PayloadVariant::ConfigCompleteId(_) => {}
                        PayloadVariant::Rebooted(_) => {}
                        PayloadVariant::ModuleConfig(_) => {}
                        PayloadVariant::Channel(mut channel) => {
                            if let Some(settings) = channel.settings.as_mut() {
                                if settings.name.is_empty() {
                                    settings.name = "Default".to_string();
                                };
                                self.channels.push((channel, vec![]))
                            }
                        }
                        PayloadVariant::QueueStatus(_) => {
                            // TODO maybe show if devices in outgoing queue?
                        }
                        PayloadVariant::XmodemPacket(_) => {
                            // TODO understand what this is used for
                        }
                        PayloadVariant::Metadata(_) => {
                            // TODO could be interesting to get device_hardware value
                        }
                        PayloadVariant::MqttClientProxyMessage(_) => {}
                        PayloadVariant::FileInfo(_) => {}
                        PayloadVariant::ClientNotification(notification) => {
                            println!("Received notification: {}", notification.message);
                        }
                        PayloadVariant::DeviceuiConfig(_) => {}
                    }
                    Task::none()
                }
                MessageSent => {
                    // TODO Mark as sent in the UI, and clear the message entry
                    // Until we have some kind of queue of messages being sent pending confirmation
                    self.message = String::new();
                    Task::none()
                }
                ConnectionError(error, detail) => {
                    eprintln!("Error: {} {}", error, detail);
                    let ec = error.clone();
                    Task::perform(empty(), move |_| Message::AppError(ec.clone()))
                }
            },
            SendMessage => {
                if let Some(channel_number) = self.channel_number {
                    // TODO Add to messages in the channel for display, or wait for packet back from radio
                    // as a confirmation? Maybe add as sending status?
                    // Display it just above the text input until confirmed by arriving in channel?
                    // for now only sent to the subscription
                    let sender = self.subscription_sender.clone();
                    // TODO add an id to the message, or get it back from the subscription to be
                    // able to handle replies to it later. Get a timestamp and maybe sender id
                    // when TextSent then add to the UI list of messages, interleaved with
                    // those received using the timestamp
                    Task::perform(
                        request_send(sender.unwrap(), self.message.clone(), channel_number),
                        |_| Message::None,
                    )
                } else {
                    Task::none()
                }
            }
            MessageInput(s) => {
                self.message = s;
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
            .push(button("Devices").on_press(Navigation(NavigationMessage::DevicesList)))
            .push(text(" / "));

        header = match connection_state {
            Disconnected => header.push(text("Disconnected")),
            Connecting(name) => header.push(text(format!("Connecting to {}", name))),
            // TODO add navigation to device when viewing a channel
            Connected(name) => header.push(button(text(name))),
            Disconnecting(name) => header.push(text(format!("Disconnecting from {}", name))),
        };

        if let Some(channel_number) = self.channel_number {
            if let Some((channel, _packets)) = &self.channels.get(channel_number as usize) {
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
        if let Some(channel_number) = self.channel_number
            && let Some((_channel, packets)) = &self.channels.get(channel_number as usize)
        {
            channel_view(self, packets)
        } else {
            self.device_view()
        }
    }

    fn device_view(&self) -> Element<'static, Message> {
        let mut channels_view = Column::new();
        for (channel, packets) in &self.channels {
            // TODO show QR of the channel config
            let channel_row = match Role::try_from(channel.role).unwrap() {
                Disabled => break,
                Primary => Self::channel_row(true, channel, packets),
                Secondary => Self::channel_row(false, channel, packets),
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
        let settings = channel.settings.as_ref();
        // TODO handle errors
        let settings = &settings.unwrap();
        settings.name.clone()
    }

    fn channel_row(
        primary: bool,
        channel: &Channel,
        packets: &[MeshPacket],
    ) -> Row<'static, Message> {
        let mut channel_row = Row::new();
        if primary {
            channel_row = channel_row.push(text("Channel: Primary: "))
        } else {
            channel_row = channel_row.push(text("Channel Secondary "))
        }

        let name = Self::channel_name(channel);
        channel_row = channel_row.push(text(name).shaping(text::Shaping::Advanced));
        channel_row = channel_row.push(text(format!(" ({})", packets.len())));
        channel_row =
            channel_row.push(button(" Chat").on_press(Message::Device(ShowChannel(channel.index))));

        channel_row
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
