use crate::channel_id::ChannelId;
use crate::channel_id::ChannelId::Node;
use crate::channel_view_entry::MCMessage;
use crate::channel_view_entry::MCMessage::{
    AlertMessage, EmojiReply, NewTextMessage, TextMessageReply,
};
use crate::mesht::device_subscription::DeviceState::{Connected, Disconnected};
use crate::mesht::device_subscription::SubscriberMessage::{
    Connect, Disconnect, RadioPacket, SendEmojiReply, SendInfo, SendPosition, SendText,
};
use crate::mesht::device_subscription::SubscriptionEvent::{
    ConnectedEvent, ConnectingEvent, ConnectionError, DeviceBatteryLevel, DisconnectedEvent,
    DisconnectingEvent, MCMessageReceived, MessageACK, MyNodeNum, NewChannel, NewNode, NewNodeInfo,
    NewNodePosition, RadioNotification,
};
use crate::{MCChannel, MCNodeInfo, MCPosition, MCUser};
use futures::SinkExt;
use futures::executor::block_on;
use iced::stream;
use meshtastic::api::{ConnectedStreamApi, StreamApi};
use meshtastic::errors::Error;
use meshtastic::packet::{PacketReceiver, PacketRouter};
use meshtastic::protobufs::config::device_config::Role;
use meshtastic::protobufs::from_radio::PayloadVariant;
use meshtastic::protobufs::from_radio::PayloadVariant::{
    ClientNotification, MyInfo, NodeInfo, Packet,
};
use meshtastic::protobufs::mesh_packet::PayloadVariant::Decoded;
use meshtastic::protobufs::telemetry::Variant::DeviceMetrics;
use meshtastic::protobufs::{FromRadio, MeshPacket, PortNum, Position, Telemetry, User};
use meshtastic::types::NodeId;
use meshtastic::utils::stream::BleId;
use meshtastic::{Message, utils};
use std::pin::Pin;
use std::time::Duration;
use tokio::sync::mpsc::{Sender, channel};
use tokio_stream::wrappers::UnboundedReceiverStream;
use tokio_stream::{Stream, StreamExt};

/// Messages sent from the subscription to the GUI
#[derive(Debug, Clone)]
pub enum SubscriptionEvent {
    /// A message from the subscription to indicate it is ready to receive messages
    Ready(Sender<SubscriberMessage>),
    ConnectedEvent(String),
    ConnectingEvent(String),
    DisconnectingEvent(String),
    DisconnectedEvent(String),
    ConnectionError(String, String, String),
    NotReady,
    MyNodeNum(u32),
    NewChannel(MCChannel),
    NewNode(MCNodeInfo),
    RadioNotification(String),
    MessageACK(ChannelId, u32),
    MCMessageReceived(ChannelId, u32, u32, MCMessage), // channel, id, from, MCMessage
    NewNodeInfo(ChannelId, u32, u32, MCUser),          // channel_id, id, from, MCUser
    NewNodePosition(ChannelId, u32, u32, MCPosition),  // channel_id, id, from, MCPosition
    DeviceBatteryLevel(Option<u32>),
}

/// A message type sent from the UI to the subscriber
pub enum SubscriberMessage {
    Connect(String),
    Disconnect,
    SendText(String, ChannelId, Option<u32>), // Optional reply to message id
    SendEmojiReply(String, ChannelId, u32),
    SendPosition(ChannelId, MCPosition),
    SendInfo(ChannelId),
    RadioPacket(Box<FromRadio>),
}

enum DeviceState {
    Disconnected,
    Connected(String, PacketReceiver),
}

struct MyRouter {
    gui_sender: futures_channel::mpsc::Sender<SubscriptionEvent>,
    my_node_num: Option<u32>,
    my_user: User,
}

impl MyRouter {
    /// Create a ny [MyRouter] with the sender to use to send events to the GUI
    /// Initialize it with unknown user data that won't be valid until we learn our own node if
    /// and then receive a [NodeInfo] with our node_id
    fn new(gui_sender: futures_channel::mpsc::Sender<SubscriptionEvent>) -> Self {
        MyRouter {
            gui_sender,
            my_node_num: None,
            my_user: User {
                id: "Unknown".to_string(),
                long_name: "Unknown".to_string(),
                short_name: "UNKN".to_string(),
                #[allow(deprecated)]
                macaddr: vec![],
                hw_model: 0,
                is_licensed: false,
                role: Role::Client as i32,
                public_key: vec![],
                is_unmessagable: Some(true),
            },
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

    /// Handle [FromRadio] packets received from the radio, filter down to packets we know the App/Gui
    /// is interested in and forward those to the Gui using the provided `gui_sender`
    async fn handle_a_packet_from_radio(&mut self, packet: Box<FromRadio>) {
        match packet.payload_variant.as_ref() {
            Some(Packet(mesh_packet)) => {
                self.handle_a_mesh_packet(mesh_packet).await;
            }
            Some(MyInfo(my_node_info)) => {
                // Capture my own node number in the router for later use
                self.my_node_num = Some(my_node_info.my_node_num);

                self.gui_sender
                    .send(MyNodeNum(my_node_info.my_node_num))
                    .await
                    .unwrap_or_else(|e| eprintln!("Send error: {e}"));
            }
            // Information about a Node that exists on the radio - which could be myself
            Some(NodeInfo(node_info)) => {
                // Once I know my own node id number, then I can capture my own node's [User] info
                if Some(node_info.num) == self.my_node_num
                    && let Some(user) = &node_info.user
                {
                    self.my_user = user.clone();
                }

                self.gui_sender
                    .send(NewNode(MCNodeInfo::from(node_info)))
                    .await
                    .unwrap_or_else(|e| eprintln!("Send error: {e}"));
            }
            // This Packet conveys information about a Channel that exists on the radio
            Some(PayloadVariant::Channel(channel)) => {
                if meshtastic::protobufs::channel::Role::try_from(channel.role)
                    != Ok(meshtastic::protobufs::channel::Role::Disabled)
                {
                    self.gui_sender
                        .send(NewChannel(MCChannel::from(channel)))
                        .await
                        .unwrap_or_else(|e| eprintln!("Send error: {e}"));
                }
            }
            Some(ClientNotification(notification)) => {
                // A notification message from the device to the client To be used for important
                // messages that should to be displayed to the user in the form of push
                // notifications or validation messages when saving invalid configuration.
                self.gui_sender
                    .send(RadioNotification(notification.message.clone()))
                    .await
                    .unwrap_or_else(|e| eprintln!("Send error: {e}"));
            }
            _ => {}
        }
    }

    /// Handle a packet we have received from the mesh, depending on the payload variant and portnum
    async fn handle_a_mesh_packet(&mut self, mesh_packet: &MeshPacket) {
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

                    self.gui_sender
                        .send(MessageACK(channel_id, data.request_id))
                        .await
                        .unwrap_or_else(|e| eprintln!("Send error: {e}"));
                }
                Ok(PortNum::AlertApp) => {
                    let channel_id = self.channel_id_from_packet(mesh_packet);
                    let message = AlertMessage(String::from_utf8(data.payload.clone()).unwrap());

                    self.gui_sender
                        .send(MCMessageReceived(
                            channel_id,
                            mesh_packet.id,
                            mesh_packet.from,
                            message,
                        ))
                        .await
                        .unwrap_or_else(|e| eprintln!("Send error: {e}"));
                }
                Ok(PortNum::TextMessageApp) => {
                    let channel_id = self.channel_id_from_packet(mesh_packet);

                    let message = if data.reply_id == 0 {
                        NewTextMessage(String::from_utf8(data.payload.clone()).unwrap())
                    } else {
                        // Emoji reply to an earlier message
                        if data.emoji == 0 {
                            // Text reply to an earlier message
                            TextMessageReply(
                                data.reply_id,
                                String::from_utf8(data.payload.clone()).unwrap(),
                            )
                        } else {
                            EmojiReply(
                                data.reply_id,
                                String::from_utf8(data.payload.clone()).unwrap(),
                            )
                        }
                    };
                    self.gui_sender
                        .send(MCMessageReceived(
                            channel_id,
                            data.request_id,
                            data.source,
                            message,
                        ))
                        .await
                        .unwrap_or_else(|e| eprintln!("Send error: {e}"));
                }
                Ok(PortNum::PositionApp) => {
                    let channel_id = self.channel_id_from_packet(mesh_packet);
                    let position: MCPosition =
                        (&Position::decode(&data.payload as &[u8]).unwrap()).into();

                    self.gui_sender
                        .send(NewNodePosition(
                            channel_id,
                            mesh_packet.id,
                            mesh_packet.from,
                            position,
                        ))
                        .await
                        .unwrap_or_else(|e| eprintln!("Send error: {e}"));
                }
                Ok(PortNum::TelemetryApp) => {
                    if let Ok(telemetry) = Telemetry::decode(&data.payload as &[u8])
                        && Some(mesh_packet.from) == self.my_node_num
                        && let Some(DeviceMetrics(metrics)) = telemetry.variant
                    {
                        self.gui_sender
                            .send(DeviceBatteryLevel(metrics.battery_level))
                            .await
                            .unwrap_or_else(|e| eprintln!("Send error: {e}"));
                    }
                }
                Ok(PortNum::NeighborinfoApp) => println!("Neighbor Info payload"),
                Ok(PortNum::NodeinfoApp) => {
                    let user: MCUser = (&User::decode(&data.payload as &[u8]).unwrap()).into();
                    let channel_id = self.channel_id_from_packet(mesh_packet);
                    self.gui_sender
                        .send(NewNodeInfo(
                            channel_id,
                            mesh_packet.id,
                            mesh_packet.from,
                            user,
                        ))
                        .await
                        .unwrap_or_else(|e| eprintln!("Send error: {e}"));
                }
                Ok(_) => {}
                _ => eprintln!("Error decoding payload portnum: {}", data.portnum),
            }
        }
    }
}

impl PacketRouter<(), Error> for MyRouter {
    fn handle_packet_from_radio(&mut self, packet: FromRadio) -> Result<(), Error> {
        block_on(self.handle_a_packet_from_radio(Box::new(packet)));
        Ok(())
    }

    fn handle_mesh_packet(&mut self, packet: MeshPacket) -> Result<(), Error> {
        block_on(self.handle_a_mesh_packet(&packet));
        Ok(())
    }

    fn source_node_id(&self) -> NodeId {
        NodeId::from(self.my_node_num.unwrap_or(0))
    }
}

/// A stream of [DeviceViewMessage] announcing the discovery or loss of devices via BLE
///
pub fn subscribe() -> impl Stream<Item = SubscriptionEvent> {
    stream::channel(
        100,
        move |mut gui_sender: futures_channel::mpsc::Sender<SubscriptionEvent>| async move {
            let mut device_state = Disconnected;
            let mut stream_api: Option<ConnectedStreamApi> = None;
            let mut my_router = MyRouter::new(gui_sender.clone());
            let (subscriber_sender, mut subscriber_receiver) = channel::<SubscriberMessage>(100);

            // Send the event sender back to the GUI, so it can send messages
            let _ = gui_sender
                .send(SubscriptionEvent::Ready(subscriber_sender.clone()))
                .await;

            // Convert the channels to a `Stream`.
            let mut subscriber_receiver = Box::pin(async_stream::stream! {
                  while let Some(item) = subscriber_receiver.recv().await {
                      yield item;
                  }
            })
                as Pin<Box<dyn Stream<Item = SubscriberMessage> + Send>>;

            loop {
                match device_state {
                    Disconnected => {
                        // Wait for a message from the UI to request that we connect to a device
                        // No need to wait for any messages from a radio, as we are not connected to one
                        if let Some(Connect(ble_device)) = subscriber_receiver.next().await {
                            gui_sender
                                .send(ConnectingEvent(ble_device.clone()))
                                .await
                                .unwrap_or_else(|e| eprintln!("Send error: {e}"));

                            match do_connect(&ble_device).await {
                                Ok((packet_receiver, stream)) => {
                                    device_state = Connected(ble_device.clone(), packet_receiver);
                                    stream_api = Some(stream);

                                    gui_sender
                                        .send(ConnectedEvent(ble_device))
                                        .await
                                        .unwrap_or_else(|e| eprintln!("Send error: {e}"));
                                }
                                Err(e) => {
                                    gui_sender
                                        .send(ConnectionError(
                                            ble_device.clone(),
                                            format!("Failed to connect to {}", ble_device),
                                            e.to_string(),
                                        ))
                                        .await
                                        .unwrap_or_else(|e| eprintln!("Send error: {e}"));
                                }
                            }
                        }
                    }
                    Connected(ble_device, packet_receiver) => {
                        let radio_stream = UnboundedReceiverStream::from(packet_receiver)
                            .map(|fr| RadioPacket(Box::new(fr)));

                        let mut merged_stream = radio_stream.merge(&mut subscriber_receiver);

                        while let Some(message) = StreamExt::next(&mut merged_stream).await {
                            let result = match message {
                                Connect(_) => {
                                    eprintln!("Cannot connect while already connected");
                                    Ok(())
                                }
                                Disconnect => break,
                                SendText(text, channel_id, reply_to_id) => {
                                    let mut api = stream_api.take().unwrap();
                                    let r = send_text_message(
                                        &mut api,
                                        &mut my_router,
                                        channel_id,
                                        reply_to_id,
                                        text,
                                    )
                                    .await;
                                    let _none = stream_api.replace(api);
                                    r
                                }
                                SendPosition(channel_id, mcposition) => {
                                    let mut api = stream_api.take().unwrap();
                                    let r = send_position(
                                        &mut api,
                                        &mut my_router,
                                        channel_id,
                                        mcposition.into(),
                                    )
                                    .await;
                                    let _none = stream_api.replace(api);
                                    r
                                }
                                SendInfo(channel_id) => {
                                    let mut api = stream_api.take().unwrap();
                                    let r = send_info(&mut api, &mut my_router, channel_id).await;
                                    let _none = stream_api.replace(api);
                                    r
                                }
                                RadioPacket(packet) => {
                                    my_router.handle_a_packet_from_radio(packet).await;
                                    Ok(())
                                }
                                SendEmojiReply(emoji, channel_id, reply_to_id) => {
                                    let mut api = stream_api.take().unwrap();
                                    let r = send_emoji_reply(
                                        &mut api,
                                        &mut my_router,
                                        channel_id,
                                        reply_to_id,
                                        emoji,
                                    )
                                    .await;
                                    let _none = stream_api.replace(api);
                                    r
                                }
                            };

                            if let Err(e) = result {
                                gui_sender
                                    .send(ConnectionError(
                                        ble_device.clone(),
                                        "Send error".to_string(),
                                        e.to_string(),
                                    ))
                                    .await
                                    .unwrap_or_else(|e| eprintln!("Send error: {e}"));
                            }
                        }

                        // Disconnect
                        gui_sender
                            .send(DisconnectingEvent(ble_device.clone()))
                            .await
                            .unwrap_or_else(|e| eprintln!("Send error: {e}"));

                        let api = stream_api.take().unwrap();
                        device_state = Disconnected;
                        let _ = do_disconnect(api).await;
                        gui_sender
                            .send(DisconnectedEvent(ble_device))
                            .await
                            .unwrap_or_else(|e| eprintln!("Send error: {e}"));
                    }
                }
            }
        },
    )
}

/// Send a Text Message to the other node or the channel, which is possibly a reply
async fn send_text_message(
    stream_api: &mut ConnectedStreamApi,
    my_router: &mut MyRouter,
    channel_id: ChannelId,
    reply_to_id: Option<u32>,
    text: String,
) -> Result<(), Error> {
    let (packet_destination, mesh_channel) = channel_id.to_destination();

    stream_api
        .send_mesh_packet(
            my_router,
            text.into_bytes().into(),
            PortNum::TextMessageApp,
            packet_destination,
            mesh_channel,
            true, // want_ack
            false,
            true, // echo_response - via PacketRouter
            reply_to_id,
            None, // Used for emoji reply! https://github.com/andrewdavidmackenzie/meshchat/issues/91
        )
        .await
}

/// Send an emoji reply
async fn send_emoji_reply(
    stream_api: &mut ConnectedStreamApi,
    my_router: &mut MyRouter,
    channel_id: ChannelId,
    reply_to_id: u32,
    emoji: String,
) -> Result<(), Error> {
    let (packet_destination, mesh_channel) = channel_id.to_destination();

    stream_api
        .send_mesh_packet(
            my_router,
            emoji.into_bytes().into(),
            PortNum::TextMessageApp,
            packet_destination,
            mesh_channel,
            false, // want_ack
            false,
            true, // echo_response - via PacketRouter
            Some(reply_to_id),
            Some(reply_to_id),
        )
        .await
}

/// Send a [Position] message to the channel or other node
async fn send_position(
    stream_api: &mut ConnectedStreamApi,
    my_router: &mut MyRouter,
    channel_id: ChannelId,
    position: Position,
) -> Result<(), Error> {
    let (packet_destination, mesh_channel) = channel_id.to_destination();
    stream_api
        .send_mesh_packet(
            my_router,
            position.encode_to_vec().into(),
            PortNum::PositionApp,
            packet_destination,
            mesh_channel,
            true, // want_ack
            false,
            true, // echo_response - via PacketRouter
            None,
            None,
        )
        .await
}

/// Send a [User] info "ping" message to the channel or other node
async fn send_info(
    stream_api: &mut ConnectedStreamApi,
    my_router: &mut MyRouter,
    channel_id: ChannelId,
) -> Result<(), Error> {
    let (packet_destination, mesh_channel) = channel_id.to_destination();

    stream_api
        .send_mesh_packet(
            my_router,
            my_router.my_user.encode_to_vec().into(),
            PortNum::NodeinfoApp,
            packet_destination,
            mesh_channel,
            true, // want_ack
            false,
            true, // echo_response - via PacketRouter
            None,
            None,
        )
        .await
}

/// Connect to a specific [BleDevice] and return a [PacketReceiver] that receives messages from the
/// radio and a [ConnectedStreamApi] that can be used to send messages to the radio.
async fn do_connect(ble_device: &str) -> Result<(PacketReceiver, ConnectedStreamApi), Error> {
    let ble_id = BleId::from_mac_address(ble_device).unwrap_or(BleId::from_name(ble_device));
    let ble_stream =
        utils::stream::build_ble_stream::<BleId>(ble_id, Duration::from_secs(4)).await?;
    let stream_api = StreamApi::new();
    let (packet_receiver, stream_api) = stream_api.connect(ble_stream).await;
    let config_id = utils::generate_rand_id();
    let stream_api = stream_api.configure(config_id).await?;
    Ok((packet_receiver, stream_api))
}

/// Disconnect from the radio we are currently connected to using the [ConnectedStreamApi]
async fn do_disconnect(stream_api: ConnectedStreamApi) -> Result<StreamApi, Error> {
    stream_api.disconnect().await
}
