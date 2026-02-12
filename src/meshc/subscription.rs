use crate::SubscriberMessage::{
    Connect, Disconnect, SendEmojiReply, SendPosition, SendText, SendUser,
};
use crate::SubscriptionEvent::{
    ConnectedEvent, ConnectingEvent, ConnectionError, DisconnectedEvent,
};
use crate::meshc::subscription::DeviceState::{Connected, Disconnected};
use crate::{SubscriberMessage, SubscriptionEvent};
use futures::{SinkExt, Stream};
use iced::stream;
use meshcore_rs::MeshCore;
use std::pin::Pin;
use tokio::sync::mpsc::channel;
use tokio_stream::StreamExt;

enum DeviceState {
    Disconnected,
    Connected(String, MeshCore),
}

/// A stream of [SubscriptionEvent] for comms between the app and the radio
///
pub fn subscribe() -> impl Stream<Item = SubscriptionEvent> {
    stream::channel(
        100,
        move |mut gui_sender: futures_channel::mpsc::Sender<SubscriptionEvent>| async move {
            let mut device_state = Disconnected;
            let (subscriber_sender, mut subscriber_receiver) = channel::<SubscriberMessage>(100);

            //Inform the GUI the subscription is ready to receive messages, so it can send messages
            let _ = gui_sender
                .send(SubscriptionEvent::Ready(subscriber_sender.clone()))
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
                        if let Some(Connect(ble_device)) = gui_stream.next().await {
                            gui_sender
                                .send(ConnectingEvent(ble_device.clone()))
                                .await
                                .unwrap_or_else(|e| eprintln!("Send error: {e}"));

                            match do_connect(&ble_device).await {
                                Ok(meshcore) => {
                                    device_state = Connected(ble_device.clone(), meshcore);

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
                    Connected(ble_device, meshcore) => {
                        /*
                        let from_radio_stream =
                            UnboundedReceiverStream::from(meshcore).map(|from_radio_packet| {
                                MeshCoreRadioPacket(Box::new(from_radio_packet))
                            });

                        let mut merged_stream = from_radio_stream.merge(&mut gui_stream);
                         */

                        while let Some(message) = StreamExt::next(&mut gui_stream).await {
                            let result = match message {
                                Connect(_) => {
                                    eprintln!("Cannot connect while already connected");
                                    Ok::<(), meshcore_rs::Error>(())
                                }
                                Disconnect => break,
                                SendText(_text, _channel_id, _reply_to_id) => {
                                    println!("Send text to meshcore");
                                    /*
                                    let r = crate::mesht::subscription::send_text_message(
                                        &mut api,
                                        &mut my_router,
                                        channel_id,
                                        reply_to_id,
                                        text,
                                    )
                                    .await;
                                    r
                                     */
                                    Ok(())
                                }
                                SendPosition(_channel_id, _mcposition) => {
                                    println!("Send position to meshcore");
                                    /*
                                    let r = crate::mesht::subscription::send_position(
                                        &mut api,
                                        &mut my_router,
                                        channel_id,
                                        mcposition.into(),
                                    )
                                    .await;
                                    r
                                     */
                                    Ok(())
                                }
                                SendUser(_channel_id, _mcuser) => {
                                    println!("Send user to meshcore");
                                    /*
                                    let r = crate::mesht::subscription::send_user(
                                        &mut api,
                                        &mut my_router,
                                        channel_id,
                                        mcuser.into(),
                                    )
                                    .await;
                                    r
                                     */
                                    Ok(())
                                }
                                SendEmojiReply(_emoji, _channel_id, _reply_to_id) => {
                                    println!("Send emoji reply to meshcore");
                                    /*
                                    let r = crate::mesht::subscription::send_emoji_reply(
                                        &mut api,
                                        &mut my_router,
                                        channel_id,
                                        reply_to_id,
                                        emoji,
                                    )
                                    .await;
                                    r
                                     */
                                    Ok(())
                                }
                                SubscriberMessage::MeshCoreRadioPacket => {
                                    // my_router.handle_a_packet_from_radio(packet).await;
                                    Ok(())
                                }
                                #[allow(unreachable_patterns)]
                                _ => Ok(()),
                            };

                            if let Err(e) = result {
                                gui_sender
                                    .send(ConnectionError(
                                        ble_device.clone(),
                                        "Subscription Error".to_string(),
                                        e.to_string(),
                                    ))
                                    .await
                                    .unwrap_or_else(|e| eprintln!("Send error: {e}"));
                            }
                        }

                        // Disconnect
                        device_state = Disconnected;
                        let _ = do_disconnect(meshcore).await;
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

/*
// Send APPSTART to initialize connection and get device info
let self_info = meshcore.commands().lock().await.send_appstart().await?;
println!("Connected to device: {}", self_info.name);
println!("  Public key: {:02x?}", &self_info.public_key[..6]);
println!("  TX power: {}", self_info.tx_power);
println!(
    "  Location: {:.6}, {:.6}",
    self_info.adv_lat as f64 / 1_000_000.0,
    self_info.adv_lon as f64 / 1_000_000.0
);

// Get battery info
let battery = meshcore.commands().lock().await.get_bat().await?;
println!("  Battery: {}%", battery.level);

// Get contacts (use longer timeout for BLE - contacts can take a while)
println!("\nFetching contacts...");
let contacts = meshcore
.commands()
.lock()
.await
.get_contacts_with_timeout(0, std::time::Duration::from_secs(30))
.await?;
println!("Found {} contacts:", contacts.len());

for contact in &contacts {
println!(
    "  - {} (prefix: {})",
    contact.adv_name,
    contact.prefix_hex()
);
}

// Subscribe to incoming messages
println!("\nListening for messages (press Ctrl+C to exit)...");

let _sub = meshcore
.subscribe(
EventType::ContactMsgRecv,
std::collections::HashMap::new(),
|event| {
if let meshcore_rs::events::EventPayload::Message(msg) = event.payload {
println!(
"Received message from {:02x?}: {}",
msg.sender_prefix, msg.text
);
}
},
)
.await;

// Start auto-fetching messages
meshcore.start_auto_message_fetching().await;
*/

/// Connect to a specific BlueTooth device by name and return a [MeshCore] that receives messages
/// from the radio and can be used to send messages to the radio.
async fn do_connect(ble_device: &str) -> meshcore_rs::Result<MeshCore> {
    MeshCore::ble_connect(ble_device).await
}

/// Disconnect from the radio we are currently connected to using the [MeshCore]
async fn do_disconnect(meshcore: MeshCore) -> meshcore_rs::Result<()> {
    meshcore.disconnect().await
}
