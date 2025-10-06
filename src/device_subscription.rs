use crate::device_subscription::DeviceState::{Connected, Disconnected};
use crate::device_subscription::SubscriberMessage::Connect;
use crate::device_subscription::SubscriptionEvent::{
    ConnectedEvent, DevicePacket, DisconnectedEvent,
};
use anyhow::Context;
use iced::futures::channel::mpsc;
use iced::futures::channel::mpsc::Sender;
use iced::futures::{SinkExt, Stream, StreamExt};
use iced::stream;
use meshtastic::api::{ConnectedStreamApi, StreamApi};
use meshtastic::packet::PacketReceiver;
use meshtastic::protobufs::FromRadio;
use meshtastic::utils;
use meshtastic::utils::stream::BleId;
use std::time::Duration;

#[derive(Debug, Clone)]
pub enum SubscriptionEvent {
    /// A message from the subscription to indicate it is ready to receive messages
    Ready(Sender<SubscriberMessage>),
    ConnectedEvent(BleId),
    DisconnectedEvent(BleId),
    DevicePacket(Box<FromRadio>),
}

/// A message type sent from the UI to the subscriber
pub enum SubscriberMessage {
    Connect(BleId),
    Disconnect,
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

        let (subscriber_sender, mut subscriber_receiver) = mpsc::channel::<SubscriberMessage>(100);

        // Send the event sender back to the GUI, so it can send messages
        let _ = gui_sender
            .send(SubscriptionEvent::Ready(subscriber_sender.clone()))
            .await;

        loop {
            match &mut device_state {
                Disconnected => {
                    // Wait for a message from the UI to request that we connect to a device
                    if let Some(Connect(id)) = subscriber_receiver.next().await {
                        let (packet_receiver, stream) = do_connect(&id).await.unwrap();
                        device_state = Connected(id.clone(), packet_receiver);
                        stream_api = Some(stream);

                        gui_sender
                            .send(ConnectedEvent(id.clone()))
                            .await
                            .unwrap_or_else(|e| eprintln!("Send error: {e}"));
                    }
                }
                Connected(id, packet_receiver) => {
                    while let Some(packet) = packet_receiver.recv().await {
                        // TODO filter out all the types that we know the GUI is not interested in

                        gui_sender
                            .send(DevicePacket(Box::new(packet)))
                            .await
                            .unwrap_or_else(|e| eprintln!("Send error: {e}"));
                    }

                    let api = stream_api.take().unwrap();
                    let _ = do_disconnect(api).await;
                    gui_sender
                        .send(DisconnectedEvent(id.clone()))
                        .await
                        .unwrap_or_else(|e| eprintln!("Send error: {e}"));
                    device_state = Disconnected;
                }
            }
        }
    })
}

async fn do_connect(id: &BleId) -> Result<(PacketReceiver, ConnectedStreamApi), anyhow::Error> {
    println!("Connecting to {}", id);
    let ble_stream = utils::stream::build_ble_stream(id, Duration::from_secs(4)).await?;
    let stream_api = StreamApi::new();
    let (packet_receiver, stream_api) = stream_api.connect(ble_stream).await;
    let config_id = utils::generate_rand_id();
    let stream_api = stream_api.configure(config_id).await?;
    Ok((packet_receiver, stream_api))
}

async fn do_disconnect(stream_api: ConnectedStreamApi) -> Result<(), anyhow::Error> {
    println!("Disconnecting");
    stream_api
        .disconnect()
        .await
        .context("Failed to disconnect")?;
    Ok(())
}
