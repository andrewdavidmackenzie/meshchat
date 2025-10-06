use crate::device_subscription::SubscriberMessage::{Connect, Disconnect};
use crate::device_subscription::SubscriptionEvent::{
    ConnectedEvent, DevicePacket, DisconnectedEvent, Ready,
};
use crate::device_subscription::{SubscriberMessage, SubscriptionEvent};
use crate::device_view::ConnectionState::{Connected, Connecting, Disconnected, Disconnecting};
use crate::device_view::DeviceViewMessage::{
    ConnectRequest, DisconnectRequest, SubscriptionMessage,
};
use crate::{device_subscription, Message, NavigationMessage};
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
use meshtastic::protobufs::{Channel, NodeInfo};
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
}

pub struct DeviceView {
    pub connection_state: ConnectionState,
    subscription_sender: Option<Sender<SubscriberMessage>>, // TODO Maybe combine with Disconnected state?
    channels: Vec<Channel>, // Upto 8 - but maybe depends on firmware
    nodes: Vec<NodeInfo>,   // all nodes known to the connected radio
}

async fn request_connection(mut sender: Sender<SubscriberMessage>, id: BleId) {
    let _ = sender.send(Connect(id)).await;
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
        }
    }

    pub fn connection_state(&self) -> &ConnectionState {
        &self.connection_state
    }

    /// Return a true value to show we can show the device view, false for main to decide
    pub fn update(&mut self, device_event: DeviceViewMessage) -> Task<Message> {
        match device_event {
            ConnectRequest(id) => {
                self.connection_state = Connecting(id.clone()); // TODO make state change depend on message back from subscription
                let sender = self.subscription_sender.clone();
                Task::perform(request_connection(sender.unwrap(), id), |_| {
                    Message::Navigation(NavigationMessage::Connecting)
                })
            }
            DisconnectRequest(id) => {
                self.connection_state = Disconnecting(id.clone()); // TODO make state change depend on message back from subscription
                // Send a message to the subscription to disconnect
                let sender = self.subscription_sender.clone();
                Task::perform(request_disconnection(sender.unwrap()), |_| {
                    Message::Navigation(NavigationMessage::Back)
                })
            }
            SubscriptionMessage(device_event) => match device_event {
                ConnectedEvent(id) => {
                    self.connection_state = Connected(id);
                    Task::none()
                }
                DisconnectedEvent(_) => {
                    self.connection_state = Disconnected;
                    Task::perform(empty(), |_| Message::Navigation(NavigationMessage::Back))
                }
                Ready(sender) => {
                    self.subscription_sender = Some(sender);
                    Task::none()
                }
                DevicePacket(packet) => {
                    match packet.payload_variant.unwrap() {
                        PayloadVariant::Packet(mesh_packet) => {
                            println!("MeshPacket: {mesh_packet:?}");
                        }
                        PayloadVariant::MyInfo(my_node_info) => {
                            println!(
                                "My node number in the mesh is: {}",
                                my_node_info.my_node_num
                            );
                        }
                        PayloadVariant::NodeInfo(node_info) => {
                            // TODO: Filter out myself as I asspear in the nodes known too?
                            self.nodes.push(node_info)
                        }
                        PayloadVariant::Config(_) => {}
                        PayloadVariant::LogRecord(_) => {}
                        PayloadVariant::ConfigCompleteId(_) => {}
                        PayloadVariant::Rebooted(_) => {}
                        PayloadVariant::ModuleConfig(_) => {}
                        PayloadVariant::Channel(channel) => self.channels.push(channel),
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
            },
        }
    }

    pub fn view(&self) -> Element<'static, Message> {
        let mut main_col = Column::new();

        match &self.connection_state {
            Disconnected => {
                main_col = main_col.push(text("disconnected"));
            }

            Connecting(id) => {
                main_col = main_col.push(text(format!("connecting to : {id}")));
            }

            Connected(id) => {
                main_col = main_col.push(text(format!("connected to : {id}")));
                main_col = main_col.push(
                    button("Disconnect").on_press(Message::Device(DisconnectRequest(id.clone()))),
                );
            }
            Disconnecting(id) => {
                main_col = main_col.push(text(format!("disconnecting from : {id}")));
            }
        }

        let mut channels_view = Column::new();
        for channel in &self.channels {
            // TODO show QR of the channel config
            // TODO button to enter the channel to chat on it
            let channel_row = match Role::try_from(channel.role).unwrap() {
                Disabled => break,
                Primary => Self::channel_row(true, channel),
                Secondary => Self::channel_row(false, channel),
            };
            channels_view = channels_view.push(channel_row);
        }

        for node in &self.nodes {
            if !node.is_ignored
                && let Some(user) = &node.user
            {
                let mut node_row = Row::new();
                // TODO: display emojis in the name
                node_row = node_row.push(text(format!("User: {}", user.long_name.clone())));
                channels_view = channels_view.push(node_row);
                // TODO can add to nodes on the channel list above if channel is "populated" (not 0?)
                // TODO mark as a favourite if has is_favorite set
            }
        }

        let channel_and_user_scroll = scrollable(channels_view)
            .direction({
                let scrollbar = Scrollbar::new().width(10.0);
                scrollable::Direction::Both {
                    horizontal: scrollbar,
                    vertical: scrollbar,
                }
            })
            .width(Fill)
            .height(Fill);

        main_col = main_col.push(channel_and_user_scroll);
        main_col.into()
    }

    fn channel_row(primary: bool, channel: &Channel) -> Row<'static, Message> {
        let mut channel_row = Row::new();
        if primary {
            channel_row = channel_row.push(text("Channel: Primary: "))
        } else {
            channel_row = channel_row.push(text("Channel Secondary: "))
        }

        if let Some(settings) = &channel.settings {
            let name = if settings.name.is_empty() {
                "Default".to_string()
            } else {
                settings.name.clone()
            };
            channel_row = channel_row.push(text(name));
        }

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
