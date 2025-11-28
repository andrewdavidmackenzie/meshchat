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
    SendText(String, ChannelId),
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
}

impl MyRouter {
    /// Handle [FromRadio] packets received from the radio, filter down to packets we know the App/Gui
    /// is interested in and forward those to the Gui using the provided `gui_sender`
    fn handle_from_radio(&mut self, packet: Box<FromRadio>) -> Result<(), Error> {
        let payload_variant = packet.payload_variant.as_ref().unwrap();
        // Filter to only send packets UI is interested in
        if matches!(
            payload_variant,
            Packet(_) | MyInfo(_) | NodeInfo(_) | Channel(_) | ClientNotification(_)
        ) {
            if let MyInfo(my_info) = &payload_variant {
                self.my_node_num = Some(my_info.my_node_num);
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
        //let router = MyRouter {};

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
                    let mut my_router = MyRouter {
                        gui_sender: gui_sender.clone(),
                        my_node_num: None,
                    };

                    let radio_stream = UnboundedReceiverStream::from(packet_receiver)
                        .map(|fr| RadioPacket(Box::new(fr)));

                    let mut merged_stream = radio_stream.merge(&mut subscriber_receiver);

                    while let Some(message) = StreamExt::next(&mut merged_stream).await {
                        match message {
                            Connect(_) => eprintln!("Already connected!"),
                            Disconnect => break,
                            SendText(text, channel_id) => {
                                let mut api = stream_api.take().unwrap();
                                send_message(&mut api, &mut my_router, channel_id, text).await;
                                let _none = stream_api.replace(api);
                            }
                            SendPosition(channel_id, position) => {
                                let mut api = stream_api.take().unwrap();
                                send_position(&mut api, &mut my_router, channel_id, position).await;
                                let _none = stream_api.replace(api);
                            }
                            SendInfo(channel_id) => {
                                let mut api = stream_api.take().unwrap();
                                let user = my_user_info();
                                send_info(&mut api, &mut my_router, channel_id, user).await;
                                let _none = stream_api.replace(api);
                            }
                            RadioPacket(packet) => my_router.handle_from_radio(packet).unwrap(),
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

// TODO handle errors
async fn send_message(
    stream_api: &mut ConnectedStreamApi,
    my_router: &mut MyRouter,
    channel_id: ChannelId,
    text: String,
) {
    let (destination, channel) = channel_id.to_destination();

    let _ = stream_api
        .send_text(my_router, text, destination, true, channel)
        .await;
}

// TODO handle errors
async fn send_position(
    stream_api: &mut ConnectedStreamApi,
    my_router: &mut MyRouter,
    channel_id: ChannelId,
    position: Position,
) {
    let (destination, channel) = channel_id.to_destination();

    let _ = stream_api
        .send_position(my_router, position, destination, true, channel)
        .await;
}

fn my_user_info() -> User {
    User {
        id: "my id".to_string(),
        long_name: "me myself".to_string(),
        short_name: "me".to_string(),
        #[allow(deprecated)]
        macaddr: vec![],
        hw_model: 0,
        is_licensed: false,
        role: Role::Client as i32,
        public_key: vec![],
        is_unmessagable: Some(true),
    }
}

fn to_destination(packet_destination: PacketDestination) -> u32 {
    match packet_destination {
        PacketDestination::Broadcast => 0xffffffff,
        PacketDestination::Node(node_id) => node_id.id(),
        PacketDestination::Local => 0, // Not sure if this is correct - but shouldn't matter
    }
}

// TODO handle errors
async fn send_info(
    stream_api: &mut ConnectedStreamApi,
    my_router: &mut MyRouter,
    channel_id: ChannelId,
    user: User,
) {
    let (packet_destination, mesh_channel) = channel_id.to_destination();

    // Create a user message data payload
    let data = Data {
        portnum: PortNum::NodeinfoApp as i32,
        payload: user.encode_to_vec(),
        want_response: false,
        ..Default::default()
    };

    // Create a mesh packet for broadcast
    let mesh_packet = MeshPacket {
        to: to_destination(packet_destination),
        from: my_router.source_node_id().id(),
        channel: mesh_channel.channel(),
        id: 0, // Will be assigned by the device
        priority: mesh_packet::Priority::Default as i32,
        payload_variant: Some(mesh_packet::PayloadVariant::Decoded(data)),
        ..Default::default()
    };

    // Create the payload variant
    let payload_variant = Some(meshtastic::protobufs::to_radio::PayloadVariant::Packet(
        mesh_packet.clone(),
    ));

    // Send using the stream API's send_to_radio_packet method
    let _ = stream_api.send_to_radio_packet(payload_variant).await;

    // Inform GUI via my packet router that it was sent
    let _ = my_router.handle_mesh_packet(mesh_packet);
}

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

async fn do_disconnect(stream_api: ConnectedStreamApi) -> Result<StreamApi, Error> {
    stream_api.disconnect().await
}
