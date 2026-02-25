use crate::channel_id::ChannelId;
use crate::channel_id::ChannelId::Node;
use crate::channel_view_entry::MCMessage::{
    AlertMessage, EmojiReply, NewTextMessage, TextMessageReply,
};
use crate::device::SubscriberMessage::{
    Connect, Disconnect, MeshTasticRadioPacket, SendEmojiReply, SendPosition, SendSelfInfo,
    SendText,
};
use crate::mesht::subscription::DeviceState::{Connected, Disconnected};

use crate::channel_id;
use crate::device::SubscriptionEvent::{
    ChannelName, ConnectedEvent, ConnectingEvent, ConnectionError, DeviceBatteryLevel,
    DisconnectedEvent, MCMessageReceived, MessageACK, MyNodeNum, NewChannel, NewNode, NewNodeInfo,
    NewNodePosition, RadioNotification, SendError,
};
use crate::device::{SubscriberMessage, SubscriptionEvent};
use crate::device_list::RadioType;
use crate::meshchat::{MCChannel, MCNodeInfo, MCPosition, MeshChat};
use futures::SinkExt;
use futures::executor::block_on;
use iced::stream;
use meshtastic::api::{ConnectedStreamApi, StreamApi};
use meshtastic::errors::Error;
use meshtastic::packet::{PacketReceiver, PacketRouter};
use meshtastic::protobufs::config::PayloadVariant::Lora;
use meshtastic::protobufs::from_radio::PayloadVariant::{
    Channel, ClientNotification, Config, MyInfo, NodeInfo, Packet,
};
use meshtastic::protobufs::mesh_packet::PayloadVariant::Decoded;
use meshtastic::protobufs::telemetry::Variant::DeviceMetrics;
use meshtastic::protobufs::{FromRadio, MeshPacket, PortNum, Position, Telemetry, User};
use meshtastic::types::NodeId;
use meshtastic::utils::stream::BleId;
use meshtastic::{Message, utils};
use std::pin::Pin;
use std::time::Duration;
use tokio::sync::mpsc::channel;
use tokio::time::timeout;
use tokio_stream::wrappers::UnboundedReceiverStream;
use tokio_stream::{Stream, StreamExt};

enum DeviceState {
    Disconnected,
    Connected(String, PacketReceiver),
}

struct MyRouter {
    gui_sender: futures_channel::mpsc::Sender<SubscriptionEvent>,
    my_node_num: Option<u32>,
}

impl MyRouter {
    /// Create a ny [MyRouter] with the sender to use to send events to the GUI
    /// Initialize it with unknown user data that won't be valid until we learn our own node if
    /// and then receive a [NodeInfo] with our node_id
    fn new(gui_sender: futures_channel::mpsc::Sender<SubscriptionEvent>) -> Self {
        MyRouter {
            gui_sender,
            my_node_num: None,
        }
    }

    /// Figure out which channel we should show a message in a [MeshPacket]
    /// I.e. is a broadcast message in a channel, or a DM to/from my node.
    fn channel_id_from_packet(&mut self, mesh_packet: &MeshPacket) -> ChannelId {
        if mesh_packet.to == u32::MAX {
            // Destined for a channel
            ChannelId::Channel(mesh_packet.channel.into())
        } else {
            // Destined for a Node
            if Some(mesh_packet.from) == self.my_node_num {
                // from me to a node - put it in that node's channel
                Node(channel_id::NodeId::from(mesh_packet.to))
            } else {
                // from the other node, put it in that node's channel
                Node(channel_id::NodeId::from(mesh_packet.from))
            }
        }
    }

    fn pascal_case(s: &str) -> String {
        s.split('_')
            .map(|word| {
                let mut chars = word.chars();
                match chars.next() {
                    None => String::new(),
                    Some(first) => first
                        .to_uppercase()
                        .chain(chars.map(|c| c.to_ascii_lowercase()))
                        .collect(),
                }
            })
            .collect()
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
                    .send(MyNodeNum(my_node_info.my_node_num.into()))
                    .await
                    .unwrap_or_else(|e| eprintln!("Send error: {e}"));
            }
            // Information about a Node that exists on the radio - which could be myself
            Some(NodeInfo(node_info)) => {
                self.gui_sender
                    .send(NewNode(MCNodeInfo::from(node_info)))
                    .await
                    .unwrap_or_else(|e| eprintln!("Send error: {e}"));
            }
            // This Packet conveys information about a Channel that exists on the radio
            Some(Channel(channel)) => {
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
                    .send(RadioNotification(
                        notification.message.clone(),
                        notification.time.into(),
                    ))
                    .await
                    .unwrap_or_else(|e| eprintln!("Send error: {e}"));
            }
            Some(Config(config)) => {
                if let Some(Lora(lora_config)) = config.payload_variant.as_ref()
                    && lora_config.use_preset
                {
                    // From docs: If bandwidth is specified, do not use modem_config
                    if lora_config.bandwidth == 0 {
                        self.gui_sender
                            .send(ChannelName(
                                0,
                                Self::pascal_case(lora_config.modem_preset().as_str_name()),
                            ))
                            .await
                            .unwrap_or_else(|e| eprintln!("Send error: {e}"));
                    }
                }
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
                        ChannelId::Channel(mesh_packet.channel.into())
                    } else {
                        // To a DM to a Node
                        Node(channel_id::NodeId::from(mesh_packet.from))
                    };

                    self.gui_sender
                        .send(MessageACK(channel_id, data.request_id.into()))
                        .await
                        .unwrap_or_else(|e| eprintln!("Send error: {e}"));
                }
                Ok(PortNum::AlertApp) => {
                    if let Ok(message) = String::from_utf8(data.payload.clone()) {
                        let channel_id = self.channel_id_from_packet(mesh_packet);

                        self.gui_sender
                            .send(MCMessageReceived(
                                channel_id,
                                mesh_packet.id.into(),
                                mesh_packet.from.into(),
                                AlertMessage(message),
                                MeshChat::now(),
                            ))
                            .await
                            .unwrap_or_else(|e| eprintln!("Send error: {e}"));
                    }
                }
                Ok(PortNum::TextMessageApp) => {
                    let channel_id = self.channel_id_from_packet(mesh_packet);
                    if let Ok(message) = String::from_utf8(data.payload.clone()) {
                        let mcmessage = if data.reply_id == 0 {
                            NewTextMessage(message)
                        } else {
                            // Emoji reply to an earlier message
                            if data.emoji == 0 {
                                // Text reply to an earlier message
                                TextMessageReply(data.reply_id.into(), message)
                            } else {
                                EmojiReply(data.reply_id.into(), message)
                            }
                        };
                        self.gui_sender
                            .send(MCMessageReceived(
                                channel_id,
                                mesh_packet.id.into(),
                                mesh_packet.from.into(),
                                mcmessage,
                                MeshChat::now(),
                            ))
                            .await
                            .unwrap_or_else(|e| eprintln!("Send error: {e}"));
                    }
                }
                Ok(PortNum::PositionApp) => {
                    if let Ok(position) = Position::decode(&data.payload as &[u8]) {
                        let channel_id = self.channel_id_from_packet(mesh_packet);
                        let mcposition: MCPosition = (&position).into();

                        self.gui_sender
                            .send(NewNodePosition(
                                channel_id,
                                mesh_packet.id.into(),
                                mesh_packet.from.into(),
                                mcposition,
                                MeshChat::now(),
                            ))
                            .await
                            .unwrap_or_else(|e| eprintln!("Send error: {e}"));
                    }
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
                    if let Ok(user) = User::decode(&data.payload as &[u8]) {
                        let channel_id = self.channel_id_from_packet(mesh_packet);
                        self.gui_sender
                            .send(NewNodeInfo(
                                channel_id,
                                mesh_packet.id.into(),
                                mesh_packet.from.into(),
                                (&user).into(),
                                MeshChat::now(),
                            ))
                            .await
                            .unwrap_or_else(|e| eprintln!("Send error: {e}"));
                    }
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

/// A stream of [SubscriptionEvent] for comms between the app and the radio
///
pub fn subscribe() -> impl Stream<Item = SubscriptionEvent> {
    stream::channel(
        100,
        move |mut gui_sender: futures_channel::mpsc::Sender<SubscriptionEvent>| async move {
            let mut device_state = Disconnected;
            let mut stream_api: Option<ConnectedStreamApi> = None;
            let mut my_router = MyRouter::new(gui_sender.clone());
            let (subscriber_sender, mut subscriber_receiver) = channel::<SubscriberMessage>(100);

            //Inform the GUI the subscription is ready to receive messages, so it can send messages
            let _ = gui_sender
                .send(SubscriptionEvent::Ready(
                    subscriber_sender,
                    RadioType::Meshtastic,
                ))
                .await;

            // Convert the channels to a `Stream`.
            let mut gui_stream = Box::pin(async_stream::stream! {
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
                        if let Some(Connect(ble_device, _)) = gui_stream.next().await {
                            gui_sender
                                .send(ConnectingEvent(ble_device.clone()))
                                .await
                                .unwrap_or_else(|e| eprintln!("Send error: {e}"));

                            match do_connect(&ble_device).await {
                                Ok((packet_receiver, stream)) => {
                                    device_state = Connected(ble_device.clone(), packet_receiver);
                                    stream_api = Some(stream);

                                    gui_sender
                                        .send(ConnectedEvent(ble_device, RadioType::Meshtastic))
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
                        let from_radio_stream = UnboundedReceiverStream::from(packet_receiver).map(
                            |from_radio_packet| MeshTasticRadioPacket(Box::new(from_radio_packet)),
                        );

                        let mut merged_stream = from_radio_stream.merge(&mut gui_stream);

                        while let Some(message) = StreamExt::next(&mut merged_stream).await {
                            let result = match message {
                                Connect(_, _) => {
                                    eprintln!("Cannot connect while already connected");
                                    Ok(())
                                }
                                Disconnect => break,
                                SendText(text, channel_id, reply_to_id) => {
                                    if let Some(mut api) = stream_api.take() {
                                        let r = send_text_message(
                                            &mut api,
                                            &mut my_router,
                                            channel_id,
                                            reply_to_id.map(u32::from),
                                            text,
                                        )
                                        .await;
                                        let _none = stream_api.replace(api);
                                        r
                                    } else {
                                        Err(Error::StreamBuildError {
                                            source: Box::new(std::io::Error::new(
                                                std::io::ErrorKind::NotConnected,
                                                "Stream API not available",
                                            )),
                                            description: "Subscription".to_string(),
                                        })
                                    }
                                }
                                SendPosition(channel_id, mcposition) => {
                                    if let Some(mut api) = stream_api.take() {
                                        let r = send_position(
                                            &mut api,
                                            &mut my_router,
                                            channel_id,
                                            mcposition.into(),
                                        )
                                        .await;
                                        let _none = stream_api.replace(api);
                                        r
                                    } else {
                                        Err(Error::StreamBuildError {
                                            source: Box::new(std::io::Error::new(
                                                std::io::ErrorKind::NotConnected,
                                                "Stream API not available",
                                            )),
                                            description: "Subscription".to_string(),
                                        })
                                    }
                                }
                                SendSelfInfo(channel_id, mcuser) => {
                                    if let Some(mut api) = stream_api.take() {
                                        let r = send_user(
                                            &mut api,
                                            &mut my_router,
                                            channel_id,
                                            mcuser.into(),
                                        )
                                        .await;
                                        let _none = stream_api.replace(api);
                                        r
                                    } else {
                                        Err(Error::StreamBuildError {
                                            source: Box::new(std::io::Error::new(
                                                std::io::ErrorKind::NotConnected,
                                                "Stream API not available",
                                            )),
                                            description: "Subscription".to_string(),
                                        })
                                    }
                                }
                                SendEmojiReply(emoji, channel_id, reply_to_id) => {
                                    if let Some(mut api) = stream_api.take() {
                                        let r = send_emoji_reply(
                                            &mut api,
                                            &mut my_router,
                                            channel_id,
                                            reply_to_id.into(),
                                            emoji,
                                        )
                                        .await;
                                        let _none = stream_api.replace(api);
                                        r
                                    } else {
                                        Err(Error::StreamBuildError {
                                            source: Box::new(std::io::Error::new(
                                                std::io::ErrorKind::NotConnected,
                                                "Stream API not available",
                                            )),
                                            description: "Subscription".to_string(),
                                        })
                                    }
                                }
                                MeshTasticRadioPacket(packet) => {
                                    my_router.handle_a_packet_from_radio(packet).await;
                                    Ok(())
                                }
                                #[allow(unreachable_patterns)]
                                _ => Ok(()),
                            };

                            if let Err(e) = result {
                                gui_sender
                                    .send(SendError(
                                        "Subscription Error".to_string(),
                                        e.to_string(),
                                    ))
                                    .await
                                    .unwrap_or_else(|e| eprintln!("Send error: {e}"));
                            }
                        }

                        // Disconnect
                        #[allow(clippy::unwrap_used)]
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
async fn send_user(
    stream_api: &mut ConnectedStreamApi,
    my_router: &mut MyRouter,
    channel_id: ChannelId,
    user: User,
) -> Result<(), Error> {
    let (packet_destination, mesh_channel) = channel_id.to_destination();

    stream_api
        .send_mesh_packet(
            my_router,
            user.encode_to_vec().into(),
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
    let ble_stream = timeout(
        Duration::from_secs(30),
        utils::stream::build_ble_stream::<BleId>(ble_id, Duration::from_secs(10)),
    )
    .await
    .map_err(|_| Error::StreamBuildError {
        source: Box::new(std::io::Error::new(
            std::io::ErrorKind::TimedOut,
            "Connect timed out",
        )),
        description: "Connect".to_string(),
    })??;
    let stream_api = StreamApi::new();
    let (packet_receiver, stream_api) = stream_api.connect(ble_stream).await;
    let config_id = utils::generate_rand_id();
    let stream_api = stream_api.configure(config_id).await?;
    Ok((packet_receiver, stream_api))
}

/// Disconnect from the radio we are currently connected to using the [ConnectedStreamApi]
async fn do_disconnect(stream_api: ConnectedStreamApi) -> Result<StreamApi, Error> {
    timeout(Duration::from_secs(1), stream_api.disconnect())
        .await
        .map_err(|_| Error::StreamBuildError {
            source: Box::new(std::io::Error::new(
                std::io::ErrorKind::TimedOut,
                "Disconnect timed out",
            )),
            description: "Disconnect".to_string(),
        })?
}

#[cfg(test)]
#[allow(clippy::panic)]
mod tests {
    use super::*;
    use crate::channel_id::MessageId;
    use crate::device::TimeStamp;
    use futures_channel::mpsc;
    use meshtastic::Message;
    use meshtastic::protobufs::{
        Channel as ProtoChannel, Data, MyNodeInfo, NodeInfo as ProtoNodeInfo, Position, User,
    };

    // Helper to create a basic MeshPacket for testing
    #[allow(deprecated)]
    fn create_mesh_packet(from: u32, to: u32, channel: u32, id: u32) -> MeshPacket {
        MeshPacket {
            from,
            to,
            channel,
            id,
            rx_time: 1234567890,
            rx_snr: 0.0,
            hop_limit: 3,
            want_ack: false,
            priority: 0,
            rx_rssi: 0,
            via_mqtt: false,
            hop_start: 0,
            public_key: vec![],
            pki_encrypted: false,
            next_hop: 0,
            relay_node: 0,
            payload_variant: None,
            tx_after: 0,
            transport_mechanism: 0,
            delayed: 0,
        }
    }

    // Helper to create a MeshPacket with decoded text payload
    fn create_text_mesh_packet(
        from: u32,
        to: u32,
        channel: u32,
        id: u32,
        text: &str,
        reply_id: u32,
        emoji: u32,
    ) -> MeshPacket {
        let mut packet = create_mesh_packet(from, to, channel, id);
        packet.payload_variant = Some(Decoded(Data {
            portnum: PortNum::TextMessageApp as i32,
            payload: text.as_bytes().to_vec(),
            want_response: false,
            dest: 0,
            source: 0,
            request_id: 0,
            reply_id,
            emoji,
            bitfield: Some(0),
        }));
        packet
    }

    // Helper to create a MeshPacket with routing/ACK payload
    fn create_ack_mesh_packet(from: u32, to: u32, channel: u32, request_id: u32) -> MeshPacket {
        let mut packet = create_mesh_packet(from, to, channel, 1);
        packet.payload_variant = Some(Decoded(Data {
            portnum: PortNum::RoutingApp as i32,
            payload: vec![],
            want_response: false,
            dest: 0,
            source: 0,
            request_id,
            reply_id: 0,
            emoji: 0,
            bitfield: Some(0),
        }));
        packet
    }

    // Helper to create a MeshPacket with position payload
    #[allow(deprecated)]
    fn create_position_mesh_packet(
        from: u32,
        to: u32,
        channel: u32,
        id: u32,
        latitude_i: i32,
        longitude_i: i32,
    ) -> MeshPacket {
        let position = Position {
            latitude_i: Some(latitude_i),
            longitude_i: Some(longitude_i),
            altitude: Some(0),
            time: 0,
            location_source: 0,
            altitude_source: 0,
            timestamp: 0,
            timestamp_millis_adjust: 0,
            altitude_hae: Some(0),
            altitude_geoidal_separation: Some(0),
            pdop: 0,
            hdop: 0,
            vdop: 0,
            gps_accuracy: 0,
            ground_speed: Some(0),
            ground_track: Some(0),
            fix_quality: 0,
            fix_type: 0,
            sats_in_view: 0,
            sensor_id: 0,
            next_update: 0,
            seq_number: 0,
            precision_bits: 0,
        };
        let mut packet = create_mesh_packet(from, to, channel, id);
        packet.payload_variant = Some(Decoded(Data {
            portnum: PortNum::PositionApp as i32,
            payload: position.encode_to_vec(),
            want_response: false,
            dest: 0,
            source: 0,
            request_id: 0,
            reply_id: 0,
            emoji: 0,
            bitfield: Some(0),
        }));
        packet
    }

    // Helper to create a MeshPacket with nodeinfo payload
    #[allow(deprecated)]
    fn create_nodeinfo_mesh_packet(
        from: u32,
        to: u32,
        channel: u32,
        id: u32,
        long_name: &str,
        short_name: &str,
    ) -> MeshPacket {
        let user = User {
            id: format!("!{:08x}", from),
            long_name: long_name.to_string(),
            short_name: short_name.to_string(),
            macaddr: vec![],
            hw_model: 0,
            is_licensed: false,
            role: 0,
            public_key: vec![],
            is_unmessagable: Some(false),
        };
        let mut packet = create_mesh_packet(from, to, channel, id);
        packet.payload_variant = Some(Decoded(Data {
            portnum: PortNum::NodeinfoApp as i32,
            payload: user.encode_to_vec(),
            want_response: false,
            dest: 0,
            source: 0,
            request_id: 0,
            reply_id: 0,
            emoji: 0,
            bitfield: Some(0),
        }));
        packet
    }

    // Test MyRouter::new()
    #[test]
    fn test_my_router_new() {
        let (sender, _receiver) = mpsc::channel::<SubscriptionEvent>(10);
        let router = MyRouter::new(sender);
        assert!(router.my_node_num.is_none());
    }

    // Test PacketRouter::source_node_id() with no node num
    #[test]
    fn test_source_node_id_none() {
        let (sender, _receiver) = mpsc::channel::<SubscriptionEvent>(10);
        let router = MyRouter::new(sender);
        assert_eq!(router.source_node_id(), NodeId::from(0u32));
    }

    // Test PacketRouter::source_node_id() with node num set
    #[test]
    fn test_source_node_id_set() {
        let (sender, _receiver) = mpsc::channel::<SubscriptionEvent>(10);
        let mut router = MyRouter::new(sender);
        router.my_node_num = Some(12345);
        assert_eq!(router.source_node_id(), NodeId::from(12345u32));
    }

    // Test channel_id_from_packet - broadcast message
    #[test]
    fn test_channel_id_broadcast_channel_0() {
        let (sender, _receiver) = mpsc::channel::<SubscriptionEvent>(10);
        let mut router = MyRouter::new(sender);
        router.my_node_num = Some(1000);

        let packet = create_mesh_packet(2000, u32::MAX, 0, 1);
        let channel_id = router.channel_id_from_packet(&packet);

        assert_eq!(channel_id, ChannelId::Channel(0.into()));
    }

    #[test]
    fn test_channel_id_broadcast_channel_1() {
        let (sender, _receiver) = mpsc::channel::<SubscriptionEvent>(10);
        let mut router = MyRouter::new(sender);
        router.my_node_num = Some(1000);

        let packet = create_mesh_packet(2000, u32::MAX, 1, 1);
        let channel_id = router.channel_id_from_packet(&packet);

        assert_eq!(channel_id, ChannelId::Channel(1.into()));
    }

    #[test]
    fn test_channel_id_broadcast_channel_5() {
        let (sender, _receiver) = mpsc::channel::<SubscriptionEvent>(10);
        let mut router = MyRouter::new(sender);
        router.my_node_num = Some(1000);

        let packet = create_mesh_packet(2000, u32::MAX, 5, 1);
        let channel_id = router.channel_id_from_packet(&packet);

        assert_eq!(channel_id, ChannelId::Channel(5.into()));
    }

    // Test channel_id_from_packet - DM from me to another node
    #[test]
    fn test_channel_id_dm_from_me() {
        let (sender, _receiver) = mpsc::channel::<SubscriptionEvent>(10);
        let mut router = MyRouter::new(sender);
        router.my_node_num = Some(1000);

        // Message from me (1000) to node 2000
        let packet = create_mesh_packet(1000, 2000, 0, 1);
        let channel_id = router.channel_id_from_packet(&packet);

        // Should be in node 2000's channel (the recipient)
        assert_eq!(channel_id, Node(channel_id::NodeId::from(2000u64)));
    }

    // Test channel_id_from_packet - DM from another node to me
    #[test]
    fn test_channel_id_dm_to_me() {
        let (sender, _receiver) = mpsc::channel::<SubscriptionEvent>(10);
        let mut router = MyRouter::new(sender);
        router.my_node_num = Some(1000);

        // Message from node 2000 to me (1000)
        let packet = create_mesh_packet(2000, 1000, 0, 1);
        let channel_id = router.channel_id_from_packet(&packet);

        // Should be in node 2000's channel (the sender)
        assert_eq!(channel_id, Node(channel_id::NodeId::from(2000u32)));
    }

    // Test channel_id_from_packet - DM between two other nodes (edge case)
    #[test]
    fn test_channel_id_dm_other_nodes() {
        let (sender, _receiver) = mpsc::channel::<SubscriptionEvent>(10);
        let mut router = MyRouter::new(sender);
        router.my_node_num = Some(1000);

        // Message from node 2000 to node 3000 (not involving me)
        let packet = create_mesh_packet(2000, 3000, 0, 1);
        let channel_id = router.channel_id_from_packet(&packet);

        // Should be in node 2000's channel (the sender)
        assert_eq!(channel_id, Node(channel_id::NodeId::from(2000u32)));
    }

    // Test channel_id_from_packet with my_node_num not set
    #[test]
    fn test_channel_id_my_node_unknown() {
        let (sender, _receiver) = mpsc::channel::<SubscriptionEvent>(10);
        let mut router = MyRouter::new(sender);
        // my_node_num is None

        let packet = create_mesh_packet(2000, 3000, 0, 1);
        let channel_id = router.channel_id_from_packet(&packet);

        // Since my_node_num is None, from != my_node_num, so uses Node(from)
        assert_eq!(channel_id, Node(channel_id::NodeId::from(2000u32)));
    }

    // Test DeviceState enum
    #[test]
    fn test_device_state_disconnected() {
        let state = Disconnected;
        assert!(matches!(state, Disconnected));
    }

    // Async tests for handle_a_packet_from_radio

    #[tokio::test]
    async fn test_handle_my_info_packet() {
        let (sender, mut receiver) = mpsc::channel::<SubscriptionEvent>(10);
        let mut router = MyRouter::new(sender);

        let from_radio = FromRadio {
            id: 1,
            payload_variant: Some(MyInfo(MyNodeInfo {
                my_node_num: 12345,
                reboot_count: 0,
                min_app_version: 0,
                pio_env: String::new(),
                device_id: vec![],
                firmware_edition: 0,
                nodedb_count: 0,
            })),
        };

        router
            .handle_a_packet_from_radio(Box::new(from_radio))
            .await;

        // Check my_node_num was captured
        assert_eq!(router.my_node_num, Some(12345));

        // Check event was sent
        let event = receiver
            .try_recv()
            .expect("Failed to receive MyNodeNum event");
        match event {
            MyNodeNum(node_id) => {
                assert_eq!(node_id, channel_id::NodeId::from(12345u64))
            }
            _ => panic!("Unexpected MyNodeNum event"),
        }
    }

    #[allow(deprecated)]
    #[tokio::test]
    async fn test_handle_node_info_packet() {
        let (sender, mut receiver) = mpsc::channel::<SubscriptionEvent>(10);
        let mut router = MyRouter::new(sender);

        let from_radio = FromRadio {
            id: 1,
            payload_variant: Some(NodeInfo(ProtoNodeInfo {
                num: 2000,
                user: Some(User {
                    id: "!abc123".to_string(),
                    long_name: "Test User".to_string(),
                    short_name: "TU".to_string(),
                    hw_model: 0,
                    is_licensed: false,
                    role: 0,
                    public_key: vec![],
                    is_unmessagable: Some(false),
                    macaddr: vec![],
                }),
                position: None,
                snr: 0.0,
                last_heard: 0,
                device_metrics: None,
                channel: 0,
                via_mqtt: false,
                hops_away: Some(0),
                is_favorite: false,
                is_ignored: false,
                is_key_manually_verified: false,
            })),
        };

        router
            .handle_a_packet_from_radio(Box::new(from_radio))
            .await;

        // Check event was sent
        let event = receiver
            .try_next()
            .expect("Failed to receive NewNode event");
        assert!(matches!(event, Some(NewNode(_))));
    }

    #[allow(deprecated)]
    #[tokio::test]
    async fn test_handle_channel_packet_enabled() {
        let (sender, mut receiver) = mpsc::channel::<SubscriptionEvent>(10);
        let mut router = MyRouter::new(sender);

        let from_radio = FromRadio {
            id: 1,
            payload_variant: Some(Channel(ProtoChannel {
                index: 0,
                settings: Some(meshtastic::protobufs::ChannelSettings {
                    channel_num: 0,
                    psk: vec![],
                    name: "TestChannel".to_string(),
                    id: 0,
                    uplink_enabled: false,
                    downlink_enabled: false,
                    module_settings: None,
                }),
                role: meshtastic::protobufs::channel::Role::Primary as i32,
            })),
        };

        router
            .handle_a_packet_from_radio(Box::new(from_radio))
            .await;

        // Check event was sent
        let event = receiver
            .try_next()
            .expect("Failed to receive NewChannel event");
        assert!(matches!(event, Some(NewChannel(_))));
    }

    #[tokio::test]
    async fn test_handle_channel_packet_disabled() {
        let (mut sender, mut receiver) = mpsc::channel::<SubscriptionEvent>(10);
        let mut router = MyRouter::new(sender.clone());

        let from_radio = FromRadio {
            id: 1,
            payload_variant: Some(Channel(ProtoChannel {
                index: 0,
                settings: None,
                role: meshtastic::protobufs::channel::Role::Disabled as i32,
            })),
        };

        router
            .handle_a_packet_from_radio(Box::new(from_radio))
            .await;

        // Close sender to allow try_next to return None
        sender.close_channel();

        // No event should be sent for the disabled channel
        assert!(
            receiver.try_recv().is_err(),
            "Expected no event for disabled channel but received one"
        );
    }

    #[tokio::test]
    async fn test_handle_client_notification_packet() {
        let (sender, mut receiver) = mpsc::channel::<SubscriptionEvent>(10);
        let mut router = MyRouter::new(sender);

        let from_radio = FromRadio {
            id: 1,
            payload_variant: Some(ClientNotification(
                meshtastic::protobufs::ClientNotification {
                    reply_id: Some(0),
                    time: 1234567890,
                    level: 0,
                    message: "Test notification".to_string(),
                    payload_variant: None,
                },
            )),
        };

        router
            .handle_a_packet_from_radio(Box::new(from_radio))
            .await;

        // Check event was sent
        let event = receiver
            .try_recv()
            .expect("Failed to receive RadioNotification event");
        match event {
            RadioNotification(msg, timestamp) => {
                assert_eq!(msg, "Test notification");
                assert_eq!(timestamp, TimeStamp::from(1234567890u32));
            }
            _ => panic!("Unexpected RadioNotification event"),
        }
    }

    #[tokio::test]
    async fn test_handle_unknown_payload() {
        let (mut sender, mut receiver) = mpsc::channel::<SubscriptionEvent>(10);
        let mut router = MyRouter::new(sender.clone());

        let from_radio = FromRadio {
            id: 1,
            payload_variant: None,
        };

        router
            .handle_a_packet_from_radio(Box::new(from_radio))
            .await;

        // Close sender to allow try_next to return None
        sender.close_channel();

        // No event should be sent for the unknown payload
        assert!(
            receiver.try_recv().is_err(),
            "Expected no event for unknown payload but received one"
        );
    }

    // Tests for handle_a_mesh_packet

    #[tokio::test]
    async fn test_handle_text_message_new() {
        let (sender, mut receiver) = mpsc::channel::<SubscriptionEvent>(10);
        let mut router = MyRouter::new(sender);
        router.my_node_num = Some(1000);

        let packet = create_text_mesh_packet(2000, u32::MAX, 0, 123, "Hello world", 0, 0);
        router.handle_a_mesh_packet(&packet).await;

        let event = receiver
            .try_recv()
            .expect("Failed to receive MCMessageReceived event for new text message");
        assert!(
            matches!(&event, MCMessageReceived(channel_id, id, from, msg, _timestamp)
                if *channel_id == ChannelId::Channel(0.into()) && *id == MessageId::from(123) && *from == channel_id::NodeId::from(2000u64)
                && matches!(msg, NewTextMessage(text) if text == "Hello world")),
            "Expected MCMessageReceived with channel 0, id 123, from 2000, NewTextMessage('Hello world'), got {:?}",
            event
        );
    }

    #[tokio::test]
    async fn test_handle_text_message_reply() {
        let (sender, mut receiver) = mpsc::channel::<SubscriptionEvent>(10);
        let mut router = MyRouter::new(sender);
        router.my_node_num = Some(1000);

        // Text reply (reply_id set, emoji = 0)
        let packet = create_text_mesh_packet(2000, u32::MAX, 0, 123, "Reply text", 456, 0);
        router.handle_a_mesh_packet(&packet).await;

        let event = receiver
            .try_recv()
            .expect("Failed to receive MCMessageReceived event for text reply");
        assert!(
            matches!(&event, MCMessageReceived(_, _, _, msg, _)
                if matches!(msg, TextMessageReply(reply_id, text) if *reply_id == MessageId::from(456) && text == "Reply text")),
            "Expected MCMessageReceived with TextMessageReply(456, 'Reply text'), got {:?}",
            event
        );
    }

    #[tokio::test]
    async fn test_handle_emoji_reply() {
        let (sender, mut receiver) = mpsc::channel::<SubscriptionEvent>(10);
        let mut router = MyRouter::new(sender);
        router.my_node_num = Some(1000);

        // Emoji reply (reply_id set, emoji != 0)
        let packet = create_text_mesh_packet(2000, u32::MAX, 0, 123, "👍", 456, 1);
        router.handle_a_mesh_packet(&packet).await;

        let event = receiver
            .try_recv()
            .expect("Failed to receive MCMessageReceived event for emoji reply");
        assert!(
            matches!(&event, MCMessageReceived(_, _, _, msg, _)
                if matches!(msg, EmojiReply(reply_id, emoji) if *reply_id == MessageId::from(456) && emoji == "👍")),
            "Expected MCMessageReceived with EmojiReply(456, '👍'), got {:?}",
            event
        );
    }

    #[tokio::test]
    async fn test_handle_ack_broadcast() {
        let (sender, mut receiver) = mpsc::channel::<SubscriptionEvent>(10);
        let mut router = MyRouter::new(sender);
        router.my_node_num = Some(1000);

        // ACK for broadcast (from == to)
        let packet = create_ack_mesh_packet(2000, 2000, 0, 789);
        router.handle_a_mesh_packet(&packet).await;

        let event = receiver
            .try_recv()
            .expect("Failed to receive MessageACK event for broadcast");
        assert!(
            matches!(&event, MessageACK(channel_id, request_id)
                if *channel_id == ChannelId::Channel(0.into()) && *request_id == MessageId::from(789)),
            "Expected MessageACK(Channel(0), 789), got {:?}",
            event
        );
    }

    #[tokio::test]
    async fn test_handle_ack_dm() {
        let (sender, mut receiver) = mpsc::channel::<SubscriptionEvent>(10);
        let mut router = MyRouter::new(sender);
        router.my_node_num = Some(1000);

        // ACK for DM (from != to)
        let packet = create_ack_mesh_packet(2000, 1000, 0, 789);
        router.handle_a_mesh_packet(&packet).await;

        let event = receiver
            .try_recv()
            .expect("Failed to receive MessageACK event for DM");
        assert!(
            matches!(&event, MessageACK(channel_id, request_id)
                if *channel_id == Node(channel_id::NodeId::from(2000u32)) && *request_id == MessageId::from(789)),
            "Expected MessageACK(Node(2000), 789), got {:?}",
            event
        );
    }

    #[tokio::test]
    async fn test_handle_packet_no_payload() {
        let (mut sender, mut receiver) = mpsc::channel::<SubscriptionEvent>(10);
        let mut router = MyRouter::new(sender.clone());

        let packet = create_mesh_packet(2000, u32::MAX, 0, 1);
        router.handle_a_mesh_packet(&packet).await;

        // Close sender to allow try_next to return None
        sender.close_channel();

        // No event should be sent for a packet without a decoded payload
        assert!(
            receiver.try_recv().is_err(),
            "Expected no event for packet with no payload but received one"
        );
    }

    // Test PacketRouter trait implementation
    #[test]
    fn test_packet_router_handle_packet_from_radio() {
        let (sender, mut receiver) = mpsc::channel::<SubscriptionEvent>(10);
        let mut router = MyRouter::new(sender);

        let from_radio = FromRadio {
            id: 1,
            payload_variant: Some(MyInfo(MyNodeInfo {
                my_node_num: 99999,
                reboot_count: 0,
                min_app_version: 0,
                pio_env: String::new(),
                device_id: vec![],
                firmware_edition: 0,
                nodedb_count: 0,
            })),
        };

        let result = router.handle_packet_from_radio(from_radio);
        assert!(result.is_ok());
        assert_eq!(router.my_node_num, Some(99999));

        // Check event was sent
        let event = receiver
            .try_recv()
            .expect("Failed to receive MyNodeNum event from PacketRouter");
        let node_id = channel_id::NodeId::from(99999u64);
        match event {
            MyNodeNum(num_received) => assert_eq!(num_received, node_id),
            _ => panic!("Expected MyNodeNum event, got {:?}", event),
        }
    }

    #[test]
    fn test_packet_router_handle_mesh_packet() {
        let (sender, mut receiver) = mpsc::channel::<SubscriptionEvent>(10);
        let mut router = MyRouter::new(sender);
        router.my_node_num = Some(1000);

        let packet = create_text_mesh_packet(2000, u32::MAX, 0, 123, "Test", 0, 0);
        let result = router.handle_mesh_packet(packet);
        assert!(result.is_ok());

        let event = receiver
            .try_recv()
            .expect("Failed to receive MCMessageReceived event from PacketRouter");
        assert!(matches!(event, MCMessageReceived(_, _, _, _, _)));
    }

    // Tests for local timestamp usage (MeshChat::now() instead of radio rx_time)

    #[tokio::test]
    async fn test_text_message_uses_local_timestamp() {
        let (sender, mut receiver) = mpsc::channel::<SubscriptionEvent>(10);
        let mut router = MyRouter::new(sender);
        router.my_node_num = Some(1000);

        let before = MeshChat::now();
        let packet = create_text_mesh_packet(2000, u32::MAX, 0, 123, "Hello", 0, 0);
        router.handle_a_mesh_packet(&packet).await;
        let after = MeshChat::now();

        let event = receiver
            .try_recv()
            .expect("Expected MCMessageReceived event");
        if let MCMessageReceived(_, _, _, _, timestamp) = event {
            assert!(
                timestamp >= before && timestamp <= after,
                "Timestamp should be local time ",
            );
        } else {
            unreachable!("Expected MCMessageReceived event, got {:?}", event);
        }
    }

    #[tokio::test]
    async fn test_text_reply_uses_local_timestamp() {
        let (sender, mut receiver) = mpsc::channel::<SubscriptionEvent>(10);
        let mut router = MyRouter::new(sender);
        router.my_node_num = Some(1000);

        let before = MeshChat::now();
        let packet = create_text_mesh_packet(2000, u32::MAX, 0, 123, "Reply", 456, 0);
        router.handle_a_mesh_packet(&packet).await;
        let after = MeshChat::now();

        let event = receiver
            .try_recv()
            .expect("Expected MCMessageReceived event");
        if let MCMessageReceived(_, _, _, _, timestamp) = event {
            assert!(
                timestamp >= before && timestamp <= after,
                "Timestamp should be local time",
            );
        } else {
            unreachable!("Expected MCMessageReceived event, got {:?}", event);
        }
    }

    #[tokio::test]
    async fn test_emoji_reply_uses_local_timestamp() {
        let (sender, mut receiver) = mpsc::channel::<SubscriptionEvent>(10);
        let mut router = MyRouter::new(sender);
        router.my_node_num = Some(1000);

        let before = MeshChat::now();
        let packet = create_text_mesh_packet(2000, u32::MAX, 0, 123, "👍", 456, 1);
        router.handle_a_mesh_packet(&packet).await;
        let after = MeshChat::now();

        let event = receiver
            .try_recv()
            .expect("Expected MCMessageReceived event");
        if let MCMessageReceived(_, _, _, _, timestamp) = event {
            assert!(
                timestamp >= before && timestamp <= after,
                "Timestamp should be local time",
            );
        } else {
            unreachable!("Expected MCMessageReceived event, got {:?}", event);
        }
    }

    #[tokio::test]
    async fn test_position_update_uses_local_timestamp() {
        let (sender, mut receiver) = mpsc::channel::<SubscriptionEvent>(10);
        let mut router = MyRouter::new(sender);
        router.my_node_num = Some(1000);

        let before = MeshChat::now();
        let packet = create_position_mesh_packet(2000, u32::MAX, 0, 123, 37_774_900, -122_419_400);
        router.handle_a_mesh_packet(&packet).await;
        let after = MeshChat::now();

        let event = receiver.try_recv().expect("Expected NewNodePosition event");
        if let NewNodePosition(_, _, _, _, timestamp) = event {
            assert!(
                timestamp >= before && timestamp <= after,
                "Timestamp should be local time",
            );
        } else {
            unreachable!("Expected NewNodePosition event, got {:?}", event);
        }
    }

    #[tokio::test]
    async fn test_node_info_uses_local_timestamp() {
        let (sender, mut receiver) = mpsc::channel::<SubscriptionEvent>(10);
        let mut router = MyRouter::new(sender);
        router.my_node_num = Some(1000);

        let before = MeshChat::now();
        let packet = create_nodeinfo_mesh_packet(2000, u32::MAX, 0, 123, "TestNode", "!abcd1234");
        router.handle_a_mesh_packet(&packet).await;
        let after = MeshChat::now();

        let event = receiver.try_recv().expect("Expected NewNodeInfo event");
        if let NewNodeInfo(_, _, _, _, timestamp) = event {
            assert!(
                timestamp >= before && timestamp <= after,
                "Timestamp should be local time",
            );
        } else {
            unreachable!("Expected NewNodeInfo event, got {:?}", event);
        }
    }
}
