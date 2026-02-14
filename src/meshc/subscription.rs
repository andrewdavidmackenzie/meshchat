use crate::device::SubscriberMessage::{
    Connect, Disconnect, MeshCoreRadioPacket, SendEmojiReply, SendPosition, SendText, SendUser,
};
use crate::device::SubscriptionEvent::{
    ConnectedEvent, ConnectingEvent, ConnectionError, DeviceBatteryLevel, DisconnectedEvent,
    MyNodeNum, MyPosition, MyUserInfo, NewNode,
};
use crate::device::{SubscriberMessage, SubscriptionEvent};
use crate::device_list::RadioType;
use crate::meshc::subscription::DeviceState::{Connected, Disconnected};
use futures::{SinkExt, Stream};
use iced::stream;
use meshcore_rs::events::{BatteryInfo, Contact, EventPayload, SelfInfo};
use meshcore_rs::{EventType, MeshCore, MeshCoreEvent};
use std::collections::HashSet;
use std::pin::Pin;
use tokio::sync::mpsc::channel;
use tokio_stream::StreamExt;

enum DeviceState {
    Disconnected,
    Connected(String, MeshCore),
}

#[derive(Debug, Default)]
struct RadioCache {
    known_channels: HashSet<u8>,
}

/// A stream of [SubscriptionEvent] for comms between the app and the radio
///
pub fn subscribe() -> impl Stream<Item = SubscriptionEvent> {
    stream::channel(
        100,
        move |mut gui_sender: futures_channel::mpsc::Sender<SubscriptionEvent>| async move {
            let mut device_state = Disconnected;
            let mut radio_cache = RadioCache::default();

            let (subscriber_sender, mut subscriber_receiver) = channel::<SubscriberMessage>(100);

            //Inform the GUI the subscription is ready to receive messages, so it can send messages
            let _ = gui_sender
                .send(SubscriptionEvent::Ready(
                    subscriber_sender,
                    RadioType::MeshCore,
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
                        if let Some(Connect(ble_device)) = gui_stream.next().await {
                            gui_sender
                                .send(ConnectingEvent(ble_device.clone()))
                                .await
                                .unwrap_or_else(|e| eprintln!("Send error: {e}"));

                            match do_connect(&ble_device).await {
                                Ok(meshcore) => {
                                    device_state = Connected(ble_device.clone(), meshcore);

                                    gui_sender
                                        .send(ConnectedEvent(ble_device, RadioType::MeshCore))
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
                        match get_my_info(&meshcore, &mut gui_sender).await {
                            Ok(_) => {
                                let _ = send_advert(&meshcore).await;
                                // TODO get any pending messages???
                                let from_radio_stream =
                                    meshcore.event_stream().map(|from_radio_packet| {
                                        MeshCoreRadioPacket(Box::new(from_radio_packet))
                                    });

                                let mut merged_stream = from_radio_stream.merge(&mut gui_stream);

                                while let Some(message) = StreamExt::next(&mut merged_stream).await
                                {
                                    let result = match message {
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
                                            Ok::<(), meshcore_rs::Error>(())
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
                                        MeshCoreRadioPacket(meshcore_event) => {
                                            handle_radio_event(
                                                &mut radio_cache,
                                                &meshcore,
                                                meshcore_event,
                                                &mut gui_sender,
                                            )
                                            .await
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
                            }
                            Err(e) => {
                                gui_sender
                                    .send(ConnectionError(
                                        ble_device.clone(),
                                        "Subscription Could not get SelfInfo".to_string(),
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

async fn handle_radio_event(
    radio_cache: &mut RadioCache,
    meshcore: &MeshCore,
    meshcore_event: Box<MeshCoreEvent>,
    gui_sender: &mut futures_channel::mpsc::Sender<SubscriptionEvent>,
) -> meshcore_rs::Result<()> {
    println!("{meshcore_event:?}");
    match meshcore_event.event_type {
        EventType::NeighboursResponse => {
            if let EventPayload::Neighbours(neighbours) = meshcore_event.payload {
                println!("Neighbours: {neighbours:?}");
                for neighbor in neighbours.neighbours {
                    println!("Neighbor: {neighbor:?}");
                    //     pub pubkey: Vec<u8>,
                    // Send node info?
                }
            }
        }
        EventType::Contacts => {
            if let EventPayload::Contacts(contacts) = meshcore_event.payload {
                println!("Contacts: {contacts:?}");
            }
        }
        EventType::NewContact => {
            if let EventPayload::Contact(contact) = meshcore_event.payload {
                println!("NewContact: {contact:?}");
            }
        }
        EventType::NextContact => {
            println!("NextContact");
        }
        EventType::SelfInfo => {
            if let EventPayload::SelfInfo(self_info) = meshcore_event.payload {
                send_self_info(&self_info, gui_sender).await;
            }
        }
        EventType::DeviceInfo => {
            if let EventPayload::DeviceInfo(device_info) = meshcore_event.payload {
                println!("Device Info: {device_info:?}");
            }
        }
        EventType::Battery => {
            if let EventPayload::Battery(battery_info) = meshcore_event.payload {
                send_battery_info(&battery_info, gui_sender).await;
            }
        }
        EventType::ChannelInfo => {
            if let EventPayload::ChannelInfo(channel_info) = meshcore_event.payload {
                println!("ChannelInfo: {channel_info:?}");
                /*
                    pub channel_idx: u8,
                    /// Channel name
                    pub name: String,
                    can use to set name for channels
                self.gui_sender
                    .send(NewChannel(MCChannel::from(channel)))
                    .await
                    .unwrap_or_else(|e| eprintln!("Send error: {e}"));
                 */
            }
        }
        EventType::MsgSent => {
            if let EventPayload::MsgSent(msg_sent) = meshcore_event.payload {
                println!("MsgSent: {msg_sent:?}");
                /*
                self.gui_sender
                    .send(MessageACK(channel_id, data.request_id))
                    .await
                    .unwrap_or_else(|e| eprintln!("Send error: {e}"));

                 */
            }
        }
        EventType::Advertisement => {
            if let EventPayload::Advertisement(advertisement) = meshcore_event.payload {
                println!("Advertisement: {advertisement:?}");
                gui_sender
                    .send(NewNode((&advertisement).into()))
                    .await
                    .unwrap_or_else(|e| eprintln!("Send error: {e}"));
            }
        }
        EventType::MessagesWaiting => {
            while let Ok(Some(message)) = meshcore.commands().lock().await.get_msg().await {
                if let Some(channel_index) = message.channel
                    && !radio_cache.known_channels.contains(&channel_index)
                {
                    if let Ok(channel_info) = meshcore
                        .commands()
                        .lock()
                        .await
                        .get_channel(channel_index)
                        .await
                    {
                        radio_cache.known_channels.insert(channel_index);
                        gui_sender
                            .send(channel_info.into())
                            .await
                            .unwrap_or_else(|e| eprintln!("Send error: {e}"));
                    }
                }

                gui_sender
                    .send(message.into())
                    .await
                    .unwrap_or_else(|e| eprintln!("Send error: {e}"));
            }
        }
        EventType::DiscoverResponse => {
            if let EventPayload::DiscoverResponse(discover_entry) = meshcore_event.payload {
                println!("DiscoverResponse: {discover_entry:?}");
                println!("Attributes: {:?}", meshcore_event.attributes);
                /*
                send node info pubkey and name?
                 */
            }
        }
        EventType::AdvertResponse => {
            if let EventPayload::AdvertResponse(advert_response) = meshcore_event.payload {
                println!("AdvertResponse: {advert_response:?}");
                /*
                send node info? name position

                    pub pubkey: [u8; 32],
                    /// Advertisement type
                    pub adv_type: u8,
                    /// Node name
                    pub node_name: String,
                    /// Latitude (optional)
                    pub lat: Option<i32>,
                    /// Longitude (optional)
                    pub lon: Option<i32>,
                    /// Node description (optional)
                    pub node_desc: Option<String>,
                 */
            }
        }
        EventType::ContactMsgRecv | EventType::ChannelMsgRecv => {
            if let EventPayload::Message(message) = meshcore_event.payload {
                gui_sender
                    .send(message.into())
                    .await
                    .unwrap_or_else(|e| eprintln!("Send error: {e}"));
            }
        }
        _ => {
            println!(
                "Event Type = {:?} ({})",
                meshcore_event.event_type, meshcore_event.event_type as u32
            );
        }
    }
    Ok(())
}

async fn send_self_info(
    self_info: &SelfInfo,
    gui_sender: &mut futures_channel::mpsc::Sender<SubscriptionEvent>,
) {
    #[allow(clippy::unwrap_used)]
    gui_sender
        .send(MyNodeNum(u32::from_be_bytes(
            self_info.public_key[0..4].try_into().unwrap(),
        ))) // TODO
        .await
        .unwrap_or_else(|e| eprintln!("Send error: {e}"));

    gui_sender
        .send(MyUserInfo(self_info.into()))
        .await
        .unwrap_or_else(|e| eprintln!("Send error: {e}"));

    gui_sender
        .send(MyPosition(self_info.into()))
        .await
        .unwrap_or_else(|e| eprintln!("Send error: {e}"));
}

async fn send_battery_info(
    battery_info: &BatteryInfo,
    gui_sender: &mut futures_channel::mpsc::Sender<SubscriptionEvent>,
) {
    println!("Battery: {battery_info:?}");
    gui_sender
        .send(DeviceBatteryLevel(Some(battery_info.level as u32)))
        .await
        .unwrap_or_else(|e| eprintln!("Send error: {e}"));
}

/// Get information about the connected device
async fn get_my_info(
    meshcore: &MeshCore,
    gui_sender: &mut futures_channel::mpsc::Sender<SubscriptionEvent>,
) -> meshcore_rs::Result<()> {
    // Send APPSTART to initialize connection and get device info
    let self_info = meshcore.commands().lock().await.send_appstart().await?;
    send_self_info(&self_info, gui_sender).await;

    // Add known nodes
    let contacts = get_contacts(meshcore).await?;
    for contact in contacts {
        gui_sender
            .send(NewNode((&contact).into()))
            .await
            .unwrap_or_else(|e| eprintln!("Send error: {e}"));
    }

    // Get battery info
    if let Ok(battery) = meshcore.commands().lock().await.get_bat().await {
        send_battery_info(&battery, gui_sender).await;
    }

    Ok(())
}

/// Advertise my presence on the network to other nodes
async fn send_advert(meshcore: &MeshCore) -> meshcore_rs::Result<()> {
    meshcore.commands().lock().await.send_advert(true).await
}

async fn get_contacts(meshcore: &MeshCore) -> meshcore_rs::Result<Vec<Contact>> {
    meshcore
        .commands()
        .lock()
        .await
        .get_contacts_with_timeout(0, std::time::Duration::from_secs(30))
        .await
}

/// Connect to a specific BlueTooth device by name and return a [MeshCore] that receives messages
/// from the radio and can be used to send messages to the radio.
async fn do_connect(ble_device: &str) -> meshcore_rs::Result<MeshCore> {
    MeshCore::ble_connect(ble_device).await
}

/// Disconnect from the radio we are currently connected to using the [MeshCore]
async fn do_disconnect(meshcore: MeshCore) -> meshcore_rs::Result<()> {
    meshcore.disconnect().await
}
