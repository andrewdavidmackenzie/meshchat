use crate::channel_view::channel_view;
use crate::device_subscription::SubscriberMessage::{Connect, Disconnect, SendText};
use crate::device_subscription::SubscriptionEvent::{
    ConnectedEvent, DevicePacket, DisconnectedEvent, Ready,
};
use crate::device_subscription::{SubscriberMessage, SubscriptionEvent};
use crate::device_view::ConnectionState::{Connected, Connecting, Disconnected, Disconnecting};
use crate::device_view::DeviceViewMessage::{
    ConnectRequest, DisconnectRequest, MessageInput, MessageSent, SendMessage, SubscriptionMessage,
};
use crate::Message::Navigation;
use crate::{device_subscription, name_from_id, Message, NavigationMessage};
use iced::futures::channel::mpsc::Sender;
use iced::futures::SinkExt;
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

#[derive(Clone)]
pub enum ConnectionState {
    Disconnected,
    Connecting(BleId),
    Connected(BleId),
    Disconnecting(BleId),
}

#[derive(Debug, Clone)]
pub enum DeviceViewMessage {
    ConnectRequest(BleId),
    DisconnectRequest(BleId),
    SubscriptionMessage(SubscriptionEvent),
    ShowChannel(i32),
    MessageInput(String),
    SendMessage,
    MessageSent,
}

pub struct DeviceView {
    connection_state: ConnectionState,
    subscription_sender: Option<Sender<SubscriberMessage>>, // TODO Maybe combine with Disconnected state?
    my_node_num: Option<u32>,
    pub(crate) channels: Vec<(Channel, Vec<MeshPacket>)>, // Upto 8 - but maybe depends on firmware
    nodes: Vec<NodeInfo>,                                 // all nodes known to the connected radio
    pub(crate) showing_channel: i32,                      // Channel numbers from 0 to 7
    pub message: String,                                  // Message typed in so far
}

async fn request_connection(mut sender: Sender<SubscriberMessage>, id: BleId) {
    let _ = sender.send(Connect(id)).await;
}

async fn request_send(mut sender: Sender<SubscriberMessage>, text: String, channel: i32) {
    let _ = sender.send(SendText(text, channel)).await;
}

async fn request_disconnection(mut sender: Sender<SubscriberMessage>) {
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
            showing_channel: -1, // No channel
            message: String::new(),
        }
    }

    pub fn connection_state(&self) -> &ConnectionState {
        &self.connection_state
    }

    /// Return a true value to show we can show the device view, false for main to decide
    pub fn update(&mut self, device_view_message: DeviceViewMessage) -> Task<Message> {
        match device_view_message {
            ConnectRequest(id) => {
                self.connection_state = Connecting(id.clone()); // TODO make state change depend on message back from subscription
                let sender = self.subscription_sender.clone();
                Task::perform(request_connection(sender.unwrap(), id), |_| {
                    Message::Navigation(NavigationMessage::DeviceView)
                })
            }
            DisconnectRequest(id) => {
                self.connection_state = Disconnecting(id.clone()); // TODO make state change depend on message back from subscription
                // Send a message to the subscription to disconnect
                let sender = self.subscription_sender.clone();
                Task::perform(request_disconnection(sender.unwrap()), |_| {
                    Message::Navigation(NavigationMessage::DevicesList)
                })
            }
            DeviceViewMessage::ShowChannel(channel_num) => {
                self.showing_channel = channel_num;
                Task::none()
            }
            SubscriptionMessage(subscription_event) => match subscription_event {
                ConnectedEvent(id) => {
                    self.connection_state = Connected(id);
                    Task::perform(empty(), |_| Message::SaveConfig)
                }
                DisconnectedEvent(_) => {
                    self.connection_state = Disconnected;
                    Task::perform(empty(), |_| {
                        Message::Navigation(NavigationMessage::DevicesList)
                    })
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
                SubscriptionEvent::MessageSent => {
                    // TODO Mark as sent in the UI
                    Task::none()
                }
                SubscriptionEvent::ConnectionError(error, detail) => {
                    eprintln!("Error: {} {}", error, detail);
                    Task::none()
                }
            },
            SendMessage => {
                println!(
                    "Sending message: {} to channel #{}",
                    self.message, self.showing_channel
                );
                // TODO Add to messages in the channel for display, or wait for packet back from radio
                // as a confirmation? Maybe add as sending status?
                // Display it just above the text input until confirmed by arriving in channel?
                // for now only sent to the subscription
                let sender = self.subscription_sender.clone();
                Task::perform(
                    request_send(sender.unwrap(), self.message.clone(), self.showing_channel),
                    |_| Message::None,
                )
            }
            MessageInput(s) => {
                self.message = s;
                Task::none()
            }
            MessageSent => {
                // TODO mark as sent in UI
                self.message = String::new();
                Task::none()
            }
        }
    }

    pub fn header<'a>(
        &'a self,
        mut header: Row<'a, Message>,
        connection_state: &ConnectionState,
    ) -> Row<'a, Message> {
        header = header
            .push(button("Devices").on_press(Navigation(NavigationMessage::DevicesList)))
            .push(text(" / "));

        match connection_state {
            Disconnected => header.push(text("Disconnected")),
            Connecting(id) => header.push(text(format!("Connecting to {}", name_from_id(id)))),
            Connected(id) => header.push(button(text(name_from_id(id)))),
            Disconnecting(id) => {
                header.push(text(format!("Disconnecting from {}", name_from_id(id))))
            }
        }
    }

    pub fn view(&self) -> Element<'static, Message> {
        if self.showing_channel >= 0 {
            // TODO add to the breadcrumbs and allow back to device view
            let (_channel, packets) = self.channels.get(self.showing_channel as usize).unwrap();

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

        let settings = channel.settings.as_ref();
        let settings = &settings.unwrap();
        let name = settings.name.clone();
        channel_row = channel_row.push(text(name).shaping(text::Shaping::Advanced));
        channel_row = channel_row.push(text(format!(" ({})", packets.len())));
        channel_row = channel_row.push(button(" Chat").on_press(Message::Device(
            DeviceViewMessage::ShowChannel(channel.index),
        )));

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
