use crate::channel_view::ChannelId;
use crate::device_subscription::DeviceState::{Connected, Disconnected};
use crate::device_subscription::SubscriberMessage::{Connect, Disconnect, RadioPacket, SendText};
use crate::device_subscription::SubscriptionEvent::{
    ConnectedEvent, ConnectionError, DeviceMeshPacket, DevicePacket, DisconnectedEvent,
};
use crate::name_from_id;
use futures::SinkExt;
use iced::stream;
use meshtastic::api::{ConnectedStreamApi, StreamApi};
use meshtastic::errors::Error;
use meshtastic::packet::{PacketDestination, PacketReceiver, PacketRouter};
use meshtastic::protobufs::from_radio::PayloadVariant::{
    Channel, ClientNotification, MyInfo, NodeInfo, Packet,
};
use meshtastic::protobufs::{FromRadio, MeshPacket};
use meshtastic::types::{MeshChannel, NodeId};
use meshtastic::utils;
use meshtastic::utils::stream::BleDevice;
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
                                        format!("Failed to connect to {}", name_from_id(&device)),
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
                                // TODO handle errors
                                match channel_id {
                                    ChannelId::Channel(channel_number) => {
                                        let _ = api
                                            .send_text(
                                                &mut my_router,
                                                text,
                                                PacketDestination::Broadcast,
                                                true,
                                                MeshChannel::from(channel_number as u32),
                                            )
                                            .await;
                                    }
                                    ChannelId::Node(node_id) => {
                                        let _ = api
                                            .send_text(
                                                &mut my_router,
                                                text.clone(),
                                                PacketDestination::Node(NodeId::from(node_id)),
                                                true,
                                                MeshChannel::default(),
                                            )
                                            .await;
                                    }
                                }
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
