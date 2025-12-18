use crate::channel_view::ChannelId;
use crate::device_subscription::DeviceState::{Connected, Disconnected};
use crate::device_subscription::SubscriberMessage::{
    Connect, Disconnect, RadioPacket, SendInfo, SendPosition, SendText,
};
use crate::device_subscription::SubscriptionEvent::{
    ConnectedEvent, ConnectionError, DeviceMeshPacket, DevicePacket, DisconnectedEvent,
};
use btleplug::api::BDAddr;
use futures::SinkExt;
use iced::stream;
use meshtastic::api::{ConnectedStreamApi, StreamApi};
use meshtastic::errors::Error;
use meshtastic::packet::{PacketReceiver, PacketRouter};
use meshtastic::protobufs::config::device_config::Role;
use meshtastic::protobufs::from_radio::PayloadVariant::{
    Channel, ClientNotification, MyInfo, NodeInfo, Packet,
};
use meshtastic::protobufs::{FromRadio, MeshPacket, PortNum, Position, User};
use meshtastic::types::NodeId;
use meshtastic::utils::stream::BleId;
use meshtastic::{Message, utils};
use std::pin::Pin;
use std::time::Duration;
use tokio::sync::mpsc::{Sender, channel};
use tokio_stream::wrappers::UnboundedReceiverStream;
use tokio_stream::{Stream, StreamExt};

#[derive(Debug, Clone)]
pub enum SubscriptionEvent {
    /// A message from the subscription to indicate it is ready to receive messages
    Ready(Sender<SubscriberMessage>),
    ConnectedEvent(BDAddr),
    DisconnectedEvent(BDAddr),
    DevicePacket(Box<FromRadio>),
    DeviceMeshPacket(Box<MeshPacket>),
    ConnectionError(BDAddr, String, String),
}

/// A message type sent from the UI to the subscriber
pub enum SubscriberMessage {
    Connect(BDAddr),
    Disconnect,
    SendText(String, ChannelId, Option<u32>), // Optional reply to message id
    SendPosition(ChannelId, Position),
    SendInfo(ChannelId),
    RadioPacket(Box<FromRadio>),
}

enum DeviceState {
    Disconnected,
    Connected(BDAddr, PacketReceiver),
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

            // Once I know my own node id number, then I can capture my own node's [User] info
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
                        if let Some(Connect(mac_address)) = subscriber_receiver.next().await {
                            match do_connect(&mac_address).await {
                                Ok((packet_receiver, stream)) => {
                                    device_state = Connected(mac_address, packet_receiver);
                                    stream_api = Some(stream);

                                    gui_sender
                                        .send(ConnectedEvent(mac_address))
                                        .await
                                        .unwrap_or_else(|e| eprintln!("Send error: {e}"));
                                }
                                Err(e) => {
                                    gui_sender
                                        .send(ConnectionError(
                                            mac_address,
                                            format!("Failed to connect to {}", mac_address),
                                            e.to_string(),
                                        ))
                                        .await
                                        .unwrap_or_else(|e| eprintln!("Send error: {e}"));
                                }
                            }
                        }
                    }
                    Connected(mac_address, packet_receiver) => {
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
                                    let r = send_position(
                                        &mut api,
                                        &mut my_router,
                                        channel_id,
                                        position,
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
                                RadioPacket(packet) => my_router.handle_from_radio(packet),
                            };

                            if let Err(e) = result {
                                gui_sender
                                    .send(ConnectionError(
                                        mac_address,
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
                            .send(DisconnectedEvent(mac_address))
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
async fn do_connect(mac_address: &BDAddr) -> Result<(PacketReceiver, ConnectedStreamApi), Error> {
    let ble_stream = utils::stream::build_ble_stream::<BleId>(
        BleId::from_mac_address(&mac_address.to_string()).unwrap(),
        Duration::from_secs(4),
    )
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
