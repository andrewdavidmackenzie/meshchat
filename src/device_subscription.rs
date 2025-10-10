use crate::device_subscription::DeviceState::{Connected, Disconnected};
use crate::device_subscription::SubscriberMessage::{Connect, Disconnect, Radio, SendText};
use crate::device_subscription::SubscriptionEvent::{
    ConnectedEvent, ConnectionError, DevicePacket, DisconnectedEvent, MessageSent,
};
use anyhow::Context;
use futures::SinkExt;
use iced::stream;
use meshtastic::api::{ConnectedStreamApi, StreamApi};
use meshtastic::packet::PacketReceiver;
use meshtastic::protobufs::from_radio::PayloadVariant::{
    Channel, ClientNotification, MyInfo, NodeInfo, Packet,
};
use meshtastic::protobufs::FromRadio;
use meshtastic::utils;
use meshtastic::utils::stream::BleId;
use std::pin::Pin;
use std::time::Duration;
use tokio::sync::mpsc::{channel, Sender};
use tokio_stream::wrappers::UnboundedReceiverStream;
use tokio_stream::{Stream, StreamExt};

#[derive(Debug, Clone)]

pub enum SubscriptionEvent {
    /// A message from the subscription to indicate it is ready to receive messages
    Ready(Sender<SubscriberMessage>),
    ConnectedEvent(BleId),
    DisconnectedEvent(BleId),
    DevicePacket(Box<FromRadio>),
    MessageSent, // Maybe add type for when we send emojis or something else
    ConnectionError(String, String),
}

/// A message type sent from the UI to the subscriber
pub enum SubscriberMessage {
    Connect(BleId),
    Disconnect,
    SendText(String, i32),
    Radio(Box<FromRadio>),
}

enum DeviceState {
    Disconnected,
    Connected(BleId, PacketReceiver),
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
                    if let Some(Connect(id)) = subscriber_receiver.next().await {
                        match do_connect(&id).await {
                            Ok((packet_receiver, stream)) => {
                                device_state = Connected(id.clone(), packet_receiver);
                                stream_api = Some(stream);

                                gui_sender
                                    .send(ConnectedEvent(id.clone()))
                                    .await
                                    .unwrap_or_else(|e| eprintln!("Send error: {e}"));
                            }
                            Err(e) => {
                                gui_sender
                                    .send(ConnectionError(
                                        format!("Failed to connect to '{id}'"),
                                        e.to_string(),
                                    ))
                                    .await
                                    .unwrap_or_else(|e| eprintln!("Send error: {e}"));
                            }
                        }
                    }
                }
                Connected(id, packet_receiver) => {
                    let radio_stream = UnboundedReceiverStream::from(packet_receiver)
                        .map(|fr| Radio(Box::new(fr)));

                    let mut merged_stream = radio_stream.merge(&mut subscriber_receiver);

                    // TODO receive either types of message: FromRadio or SubscriberMessage from the merged stream
                    // This is the code that works to handle the radio packets
                    while let Some(message) = StreamExt::next(&mut merged_stream).await {
                        match message {
                            Connect(_) => eprintln!("Already connected!"),
                            Disconnect => break,
                            SendText(text, channel_number) => {
                                println!("SendText '{text}' to channel: {channel_number}");
                                // TODO handle send errors and report to UI
                                let api = stream_api.take().unwrap();
                                /*
                                let _ = api
                                    .send_text(
                                        &mut router,
                                        text,
                                        PacketDestination::Broadcast,
                                        true,
                                        MeshChannel::from(channel_number as u32),
                                    )
                                    .await;
                                 */
                                gui_sender
                                    .send(MessageSent)
                                    .await
                                    .unwrap_or_else(|e| eprintln!("Send error: {e}"));

                                let _none = stream_api.replace(api);
                            }
                            Radio(packet) => {
                                let payload_variant = packet.payload_variant.as_ref().unwrap();
                                // Filter to only send packets UI is interested in
                                if matches!(
                                    payload_variant,
                                    Packet(_)
                                        | MyInfo(_)
                                        | NodeInfo(_)
                                        | Channel(_)
                                        | ClientNotification(_)
                                ) {
                                    gui_sender
                                        .send(DevicePacket(packet))
                                        .await
                                        .unwrap_or_else(|e| eprintln!("Send error: {e}"));
                                }
                            }
                        }
                    }

                    // Disconnect
                    let api = stream_api.take().unwrap();
                    device_state = Disconnected;
                    let _ = do_disconnect(api).await;
                    gui_sender
                        .send(DisconnectedEvent(id.clone()))
                        .await
                        .unwrap_or_else(|e| eprintln!("Send error: {e}"));
                }
            }
        }
    })
}

async fn do_connect(id: &BleId) -> Result<(PacketReceiver, ConnectedStreamApi), anyhow::Error> {
    let ble_stream = utils::stream::build_ble_stream(id, Duration::from_secs(4)).await?;
    let stream_api = StreamApi::new();
    let (packet_receiver, stream_api) = stream_api.connect(ble_stream).await;
    let config_id = utils::generate_rand_id();
    let stream_api = stream_api.configure(config_id).await?;
    Ok((packet_receiver, stream_api))
}

async fn do_disconnect(stream_api: ConnectedStreamApi) -> Result<(), anyhow::Error> {
    stream_api
        .disconnect()
        .await
        .context("Failed to disconnect")?;
    Ok(())
}
