use crate::channel_view::ChannelId;
use crate::device_name;
use crate::device_subscription::DeviceState::{Connected, Disconnected};
use crate::device_subscription::SubscriberMessage::{
    Connect, Disconnect, RadioPacket, SendInfo, SendPosition, SendText,
};
use crate::device_subscription::SubscriptionEvent::{
    ConnectedEvent, ConnectionError, DeviceMeshPacket, DevicePacket, DisconnectedEvent,
};
use futures::SinkExt;
use iced::stream;
use meshtastic::api::{ConnectedStreamApi, StreamApi};
use meshtastic::errors::Error;
use meshtastic::packet::{PacketDestination, PacketReceiver, PacketRouter};
use meshtastic::protobufs::config::device_config::Role;
use meshtastic::protobufs::from_radio::PayloadVariant::{
    Channel, ClientNotification, MyInfo, NodeInfo, Packet,
};
use meshtastic::protobufs::{Data, FromRadio, MeshPacket, PortNum, Position, User, mesh_packet};
use meshtastic::types::NodeId;
use meshtastic::utils::stream::BleDevice;
use meshtastic::{Message, utils};
use std::pin::Pin;
use std::time::Duration;
use tokio::sync::mpsc::{Sender, channel};
use tokio_stream::wrappers::UnboundedReceiverStream;
use tokio_stream::{Stream, StreamExt};

// TODO it looks like message id is not being set correctly, as a new message overwrites previous
// ones in the channel view
// I think due to change in method used to send message
// review the crate code to see how it calls the Router, where it gets message ID from

#[derive(Debug, Clone)]
pub enum SubscriptionEvent {
    /// A message from the subscription to indicate it is ready to receive messages
    Ready(Sender<SubscriberMessage>),
    ConnectedEvent(BleDevice),
    DisconnectedEvent(BleDevice),
    DevicePacket(Box<FromRadio>),
    DeviceMeshPacket(Box<MeshPacket>),
    ConnectionError(BleDevice, String, String),
}

/// A message type sent from the UI to the subscriber
pub enum SubscriberMessage {
    Connect(BleDevice),
    Disconnect,
    SendText(String, ChannelId, Option<u32>), // Optional reply to message id
    SendPosition(ChannelId, Position),
    SendInfo(ChannelId),
    RadioPacket(Box<FromRadio>),
}

enum DeviceState {
    Disconnected,
    Connected(BleDevice, PacketReceiver),
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

    /// Handle [FromRadio] packets received from the radio, filter down to packets we know the App/Gui
    /// is interested in and forward those to the Gui using the provided `gui_sender`
    fn handle_from_radio(&mut self, packet: Box<FromRadio>) -> Result<(), Error> {
        let payload_variant = packet.payload_variant.as_ref().unwrap();
        // Filter to only send packets UI is interested in
        if matches!(
            payload_variant,
            Packet(_) | MyInfo(_) | NodeInfo(_) | Channel(_) | ClientNotification(_)
        ) {
            // Capture my own node number
            if let MyInfo(my_info) = &payload_variant {
                self.my_node_num = Some(my_info.my_node_num);
            }

            // Once I know my own node id number then I can capture my own node's [User] info
            if let NodeInfo(node_info) = &payload_variant
                && Some(node_info.num) == self.my_node_num
                && let Some(user) = &node_info.user
            {
                self.my_user = user.clone();
            }

            self.gui_sender
                .try_send(DevicePacket(packet))
                .unwrap_or_else(|e| eprintln!("Send error: {e}"));
        }

        Ok(())
    }
}

impl PacketRouter<(), Error> for MyRouter {
    fn handle_packet_from_radio(&mut self, packet: FromRadio) -> Result<(), Error> {
        self.handle_from_radio(Box::new(packet))
    }

    fn handle_mesh_packet(&mut self, packet: MeshPacket) -> Result<(), Error> {
        self.gui_sender
            .try_send(DeviceMeshPacket(Box::new(packet)))
            .unwrap_or_else(|e| eprintln!("Send error: {e}"));
        Ok(())
    }

    fn source_node_id(&self) -> NodeId {
        NodeId::from(self.my_node_num.unwrap_or(0))
    }
}

/// A stream of [DeviceViewMessage] announcing the discovery or loss of devices via BLE
///
pub fn subscribe() -> impl Stream<Item = SubscriptionEvent> {
    stream::channel(100, move |mut gui_sender| async move {
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
                    if let Some(Connect(device)) = subscriber_receiver.next().await {
                        match do_connect(&device).await {
                            Ok((packet_receiver, stream)) => {
                                device_state = Connected(device.clone(), packet_receiver);
                                stream_api = Some(stream);

                                gui_sender
                                    .send(ConnectedEvent(device.clone()))
                                    .await
                                    .unwrap_or_else(|e| eprintln!("Send error: {e}"));
                            }
                            Err(e) => {
                                gui_sender
                                    .send(ConnectionError(
                                        device.clone(),
                                        format!("Failed to connect to {}", device_name(&device)),
                                        e.to_string(),
                                    ))
                                    .await
                                    .unwrap_or_else(|e| eprintln!("Send error: {e}"));
                            }
                        }
                    }
                }
                Connected(device, packet_receiver) => {
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
                            SendPosition(channel_id, position) => {
                                let mut api = stream_api.take().unwrap();
                                let r =
                                    send_position(&mut api, &mut my_router, channel_id, position)
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
                            RadioPacket(packet) => my_router.handle_from_radio(packet),
                        };

                        if let Err(e) = result {
                            gui_sender
                                .send(ConnectionError(
                                    device.clone(),
                                    "Send error".to_string(),
                                    e.to_string(),
                                ))
                                .await
                                .unwrap_or_else(|e| eprintln!("Send error: {e}"));
                        }
                    }

                    // Disconnect
                    let api = stream_api.take().unwrap();
                    device_state = Disconnected;
                    let _ = do_disconnect(api).await;
                    gui_sender
                        .send(DisconnectedEvent(device.clone()))
                        .await
                        .unwrap_or_else(|e| eprintln!("Send error: {e}"));
                }
            }
        }
    })
}

/// Send a Text Message to the other node or the channel, which is possibly a reply
async fn send_text_message(
    stream_api: &mut ConnectedStreamApi,
    my_router: &mut MyRouter,
    channel_id: ChannelId,
    reply_to_id: Option<u32>,
    text: String,
) -> Result<(), Error> {
    let data = Data {
        portnum: PortNum::TextMessageApp as i32,
        payload: text.encode_to_vec(),
        reply_id: reply_to_id.unwrap_or(0),
        ..Default::default()
    };

    send_packet(stream_api, my_router, channel_id, data).await
}

/// Send a [Position] message to the channel or other node
async fn send_position(
    stream_api: &mut ConnectedStreamApi,
    my_router: &mut MyRouter,
    channel_id: ChannelId,
    position: Position,
) -> Result<(), Error> {
    let data = Data {
        portnum: PortNum::PositionApp as i32,
        payload: position.encode_to_vec(),
        ..Default::default()
    };

    send_packet(stream_api, my_router, channel_id, data).await
}

/// Send a [User] info "ping" message to the channel or other node
async fn send_info(
    stream_api: &mut ConnectedStreamApi,
    my_router: &mut MyRouter,
    channel_id: ChannelId,
) -> Result<(), Error> {
    let data = Data {
        portnum: PortNum::NodeinfoApp as i32,
        payload: my_router.my_user.encode_to_vec(),
        ..Default::default()
    };

    send_packet(stream_api, my_router, channel_id, data).await
}

/// Send a packet to the radio on the specific channel id (Node or channel) with [Data]
async fn send_packet(
    stream_api: &mut ConnectedStreamApi,
    my_router: &mut MyRouter,
    channel_id: ChannelId,
    data: Data,
) -> Result<(), Error> {
    let (packet_destination, mesh_channel) = channel_id.to_destination();

    let to = match packet_destination {
        PacketDestination::Broadcast => 0xffffffff,
        PacketDestination::Node(node_id) => node_id.id(),
        PacketDestination::Local => 0, // Not sure if this is correct - but shouldn't matter
    };

    // Create a mesh packet for sending, always request ACK
    let mesh_packet = MeshPacket {
        from: my_router.source_node_id().id(),
        to,
        channel: mesh_channel.channel(),
        payload_variant: Some(mesh_packet::PayloadVariant::Decoded(data)),
        want_ack: true,
        ..Default::default()
    };

    // Create the payload variant
    let payload_variant = Some(meshtastic::protobufs::to_radio::PayloadVariant::Packet(
        mesh_packet.clone(),
    ));

    // Send using the stream API's send_to_radio_packet method
    stream_api.send_to_radio_packet(payload_variant).await?;

    // Inform GUI via my packet router that it was sent
    my_router.handle_mesh_packet(mesh_packet)
}

/// Connect to a specific [BleDevice] and return a [PacketReceiver] that receives messages from the
/// radio and a [ConnectedStreamApi] that can be used to send messages to the radio.
async fn do_connect(device: &BleDevice) -> Result<(PacketReceiver, ConnectedStreamApi), Error> {
    let ble_stream =
        utils::stream::build_ble_stream::<BleDevice>(device.clone(), Duration::from_secs(4))
            .await?;
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
