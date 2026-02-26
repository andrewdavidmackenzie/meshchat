use crate::conversation_id::ConversationId::{Channel, Node};
use crate::conversation_id::{ChannelIndex, ConversationId, MessageId, NodeId};
use crate::device::SubscriberMessage::{
    Connect, Disconnect, MeshCoreRadioPacket, SendEmojiReply, SendPosition, SendSelfInfo, SendText,
};
use crate::device::SubscriptionEvent::{
    ConnectedEvent, ConnectingEvent, ConnectionError, DeviceBatteryLevel, DisconnectedEvent,
    MCMessageReceived, MessageACK, MyNodeNum, MyPosition, MyUserInfo, NewChannel, NewNode,
    SendError,
};
use crate::device::{SubscriberMessage, SubscriptionEvent};
use crate::device_list::RadioType;
use crate::meshc::subscription::DeviceState::{Connected, Disconnected};
use futures::{SinkExt, Stream};
use iced::stream;
use meshcore_rs::commands::Destination;
use meshcore_rs::events::{
    BatteryInfo, ChannelInfoData, Contact, DeviceInfoData, EventPayload, NeighboursData, SelfInfo,
};
use meshcore_rs::{ChannelMessage, ContactMessage, Error, EventType, MeshCore, MeshCoreEvent};
use std::collections::{HashMap, HashSet};
use std::pin::Pin;
use tokio::sync::mpsc::channel;
use tokio_stream::StreamExt;

use crate::meshchat::{MCPosition, MCUser, MeshChat};
use crate::message::MCContent;
use tokio::time::{Duration, timeout};

enum DeviceState {
    Disconnected,
    Connected(String, MeshCore),
}

#[derive(Debug, Default)]
struct RadioCache {
    self_id: NodeId,
    self_info: SelfInfo,
    device_info: DeviceInfoData,
    known_channels: HashSet<u8>,
    /// Contact Name (String), Contact Node ID (NodeId)
    known_contacts: HashMap<String, NodeId>,
    /// Messages that have been sent (by MessageId) that are pending an ACK (ChannelId for the message)
    pending_ack: HashMap<MessageId, ConversationId>,
}

impl RadioCache {
    fn user(&self) -> MCUser {
        let node_id: NodeId = (&self.self_info.public_key).into();
        MCUser {
            id: node_id.to_string(),
            long_name: self.self_info.name.clone(),
            short_name: self.self_info.name.clone(),
            hw_model_str: self.device_info.model.clone().unwrap_or("".into()),
            ..Default::default()
        }
    }
}
/// A stream of [SubscriptionEvent] for comms between the app and the radio
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
                        if let Some(Connect(ble_device, _)) = gui_stream.next().await {
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
                        match initiate(&mut radio_cache, &meshcore, &mut gui_sender).await {
                            Ok(_) => {
                                let from_radio_stream =
                                    meshcore.event_stream().map(|from_radio_packet| {
                                        MeshCoreRadioPacket(Box::new(from_radio_packet))
                                    });

                                let mut merged_stream = from_radio_stream.merge(&mut gui_stream);

                                while let Some(message) = StreamExt::next(&mut merged_stream).await
                                {
                                    let result = match message {
                                        Disconnect => break,
                                        SendText(text, conversation_id, reply_to_message_id) => {
                                            send_text_message(
                                                &meshcore,
                                                &mut radio_cache,
                                                conversation_id,
                                                text,
                                                reply_to_message_id,
                                                &mut gui_sender,
                                            )
                                            .await
                                        }
                                        SendPosition(conversation_id, mcposition) => {
                                            send_position(
                                                &meshcore,
                                                &mut radio_cache,
                                                conversation_id,
                                                mcposition,
                                                &mut gui_sender,
                                            )
                                            .await
                                        }
                                        SendSelfInfo(conversation_id, mcuser) => {
                                            send_self_info(
                                                &meshcore,
                                                &mut radio_cache,
                                                conversation_id,
                                                mcuser,
                                                &mut gui_sender,
                                            )
                                            .await
                                        }
                                        SendEmojiReply(
                                            emoji,
                                            conversation_id,
                                            reply_to_message_id,
                                        ) => {
                                            send_emoji_reply(
                                                &meshcore,
                                                &mut radio_cache,
                                                conversation_id,
                                                emoji,
                                                reply_to_message_id,
                                                &mut gui_sender,
                                            )
                                            .await
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
                                            .send(SendError(
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

                        // Reset the cache
                        radio_cache = RadioCache::default();

                        // Suppress disconnect errors
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

/// Connect to a specific BlueTooth device by name and return a [MeshCore] that receives messages
/// from the radio and can be used to send messages to the radio.
async fn do_connect(ble_device: &str) -> meshcore_rs::Result<MeshCore> {
    timeout(Duration::from_secs(10), MeshCore::ble_connect(ble_device))
        .await
        .map_err(|_| Error::Timeout("Connect".to_string()))?
}

/// Disconnect from the radio we are currently connected to using the [MeshCore]
async fn do_disconnect(meshcore: MeshCore) -> meshcore_rs::Result<()> {
    timeout(Duration::from_secs(1), meshcore.disconnect())
        .await
        .map_err(|_| Error::Timeout("Disconnect".to_string()))?
}

/// Initiate the conversation with the radio, requesting a lot of basic information
/// we need to populate the GUI
async fn initiate(
    radio_cache: &mut RadioCache,
    meshcore: &MeshCore,
    gui_sender: &mut futures_channel::mpsc::Sender<SubscriptionEvent>,
) -> meshcore_rs::Result<()> {
    // Send APPSTART to initialise connection and get device info
    let self_info = meshcore.commands().lock().await.send_appstart().await?;
    handle_self_info(radio_cache, self_info, gui_sender).await;

    let device_info = meshcore.commands().lock().await.send_device_query().await?;
    handle_device_info(radio_cache, device_info, gui_sender).await;

    // Add known contacts
    get_contacts(meshcore, radio_cache, gui_sender).await?;

    get_channels(meshcore, gui_sender).await?;

    get_pending_messages(radio_cache, meshcore, gui_sender).await;

    // Get battery info
    if let Ok(battery) = meshcore.commands().lock().await.get_bat().await {
        handle_battery_info(&battery, gui_sender).await;
    }

    send_advert(meshcore).await?;

    Ok(())
}

/// Fetch all known channels from the radio and send them to the GUI
async fn get_channels(
    meshcore: &MeshCore,
    gui_sender: &mut futures_channel::mpsc::Sender<SubscriptionEvent>,
) -> meshcore_rs::Result<()> {
    let mut index = 0;
    while let Ok(channel) = meshcore.commands().lock().await.get_channel(index).await {
        if !channel.name.is_empty() {
            gui_sender
                .send(NewChannel(channel.into()))
                .await
                .unwrap_or_else(|e| eprintln!("Send error: {e}"));
        }
        index += 1;
    }

    Ok(())
}

/// Fetch known contacts from the radio
async fn get_contacts(
    meshcore: &MeshCore,
    radio_cache: &mut RadioCache,
    gui_sender: &mut futures_channel::mpsc::Sender<SubscriptionEvent>,
) -> meshcore_rs::Result<()> {
    let contacts = meshcore
        .commands()
        .lock()
        .await
        .get_contacts_with_timeout(0, Duration::from_secs(30))
        .await?;

    for contact in contacts {
        handle_new_contact(radio_cache, contact, gui_sender).await;
    }

    Ok(())
}

/// Fetch known neighbours from a contact
#[allow(dead_code)]
async fn get_neighbours(
    meshcore: &MeshCore,
    contact: &Contact,
) -> meshcore_rs::Result<NeighboursData> {
    meshcore
        .commands()
        .lock()
        .await
        .request_neighbours(contact, 64, 0)
        .await
}

/// Fetch any pending messages from the radio and send them to the GUI
async fn get_pending_messages(
    radio_cache: &mut RadioCache,
    meshcore: &MeshCore,
    gui_sender: &mut futures_channel::mpsc::Sender<SubscriptionEvent>,
) {
    while let Ok(Some(event)) = meshcore.commands().lock().await.get_msg().await {
        match event.event_type {
            EventType::ContactMsgRecv => {
                if let EventPayload::ContactMessage(contact_message) = event.payload {
                    handle_new_contact_message(contact_message, gui_sender).await;
                }
            }
            EventType::ChannelMsgRecv => {
                if let EventPayload::ChannelMessage(channel_message) = event.payload {
                    handle_new_channel_message(radio_cache, channel_message, gui_sender).await;
                }
            }
            _ => {}
        }
    }
}

/// Send a text message
async fn send_text_message(
    meshcore: &MeshCore,
    radio_cache: &mut RadioCache,
    conversation_id: ConversationId,
    text: String,
    _reply_to_message_id: Option<MessageId>,
    gui_sender: &mut futures_channel::mpsc::Sender<SubscriptionEvent>,
) -> meshcore_rs::Result<()> {
    match conversation_id {
        Channel(channel_index) => {
            // No message sent info returned for a channel message
            meshcore
                .commands()
                .lock()
                .await
                .send_channel_msg(channel_index.into(), &text, None)
                .await?;

            // Reflect the message back into the GUI
            let msg = MCContent::NewTextMessage(text);
            gui_sender
                .send(MCMessageReceived(
                    conversation_id,
                    MeshChat::now().into(),
                    radio_cache.self_id,
                    msg,
                    MeshChat::now(),
                ))
                .await
                .unwrap_or_else(|e| eprintln!("Send error: {e}"));
        }
        Node(node_id) => {
            let message_sent_info = meshcore
                .commands()
                .lock()
                .await
                .send_msg(<NodeId as Into<Destination>>::into(node_id), &text, None)
                .await?;

            let message_id: MessageId = message_sent_info.expected_ack.into();

            // Mark this sent message as pending an ACK
            radio_cache.pending_ack.insert(message_id, Node(node_id));

            // Reflect the message back into the GUI
            let msg = MCContent::NewTextMessage(text);
            gui_sender
                .send(MCMessageReceived(
                    conversation_id,
                    message_id,
                    radio_cache.self_id,
                    msg,
                    MeshChat::now(),
                ))
                .await
                .unwrap_or_else(|e| eprintln!("Send error: {e}"));
        }
    }

    Ok(())
}

/// Send an Emoji reply to a message - MeshCore has no way to refer back to the message that is
/// being replied to, and so it's just sent as a normal text message
async fn send_emoji_reply(
    meshcore: &MeshCore,
    radio_cache: &mut RadioCache,
    conversation_id: ConversationId,
    text: String,
    reply_to_message_id: MessageId,
    gui_sender: &mut futures_channel::mpsc::Sender<SubscriptionEvent>,
) -> meshcore_rs::Result<()> {
    send_text_message(
        meshcore,
        radio_cache,
        conversation_id,
        text,
        Some(reply_to_message_id),
        gui_sender,
    )
    .await
}

/// Send a position message
async fn send_position(
    meshcore: &MeshCore,
    radio_cache: &mut RadioCache,
    conversation_id: ConversationId,
    position: MCPosition,
    gui_sender: &mut futures_channel::mpsc::Sender<SubscriptionEvent>,
) -> meshcore_rs::Result<()> {
    let text = format!(
        "My position https://maps.google.com/?q={:.7},{:.7}",
        position.latitude, position.longitude
    );

    send_text_message(
        meshcore,
        radio_cache,
        conversation_id,
        text,
        None,
        gui_sender,
    )
    .await
}

/// Send SelfInfo to a channel or a node
async fn send_self_info(
    meshcore: &MeshCore,
    radio_cache: &mut RadioCache,
    conversation_id: ConversationId,
    user: MCUser,
    gui_sender: &mut futures_channel::mpsc::Sender<SubscriptionEvent>,
) -> meshcore_rs::Result<()> {
    let text = format!("My user info: {user}");
    send_text_message(
        meshcore,
        radio_cache,
        conversation_id,
        text,
        None,
        gui_sender,
    )
    .await
}

/// Advertise my presence on the network to other nodes
async fn send_advert(meshcore: &MeshCore) -> meshcore_rs::Result<MeshCoreEvent> {
    meshcore.commands().lock().await.send_advert(true).await
}

/// Handle reception of SelfInfo, combine with existing info and send it to the GUI
async fn handle_self_info(
    radio_cache: &mut RadioCache,
    self_info: SelfInfo,
    gui_sender: &mut futures_channel::mpsc::Sender<SubscriptionEvent>,
) {
    radio_cache.self_id = (&self_info.public_key).into();

    // update the info stored in radio_cache
    radio_cache.self_info = self_info.clone();

    gui_sender
        .send(MyNodeNum((&self_info.public_key).into()))
        .await
        .unwrap_or_else(|e| eprintln!("Send error: {e}"));

    gui_sender
        .send(MyUserInfo(radio_cache.user()))
        .await
        .unwrap_or_else(|e| eprintln!("Send error: {e}"));

    gui_sender
        .send(MyPosition((&self_info).into()))
        .await
        .unwrap_or_else(|e| eprintln!("Send error: {e}"));
}

/// Handle reception of DeviceInfo, combine with existing info and send it to the GUI
async fn handle_device_info(
    radio_cache: &mut RadioCache,
    device_info: DeviceInfoData,
    gui_sender: &mut futures_channel::mpsc::Sender<SubscriptionEvent>,
) {
    // update the info stored in radio_cache
    radio_cache.device_info = device_info.clone();

    gui_sender
        .send(MyUserInfo(radio_cache.user()))
        .await
        .unwrap_or_else(|e| eprintln!("Send error: {e}"));
}

/// Handle the reception of a new Contact and send to the GUI
async fn handle_new_contact(
    radio_cache: &mut RadioCache,
    contact: Contact,
    gui_sender: &mut futures_channel::mpsc::Sender<SubscriptionEvent>,
) {
    let node_id = (&contact.prefix()).into();
    radio_cache
        .known_contacts
        .insert(contact.adv_name.clone(), node_id);
    gui_sender
        .send(NewNode(contact.into()))
        .await
        .unwrap_or_else(|e| eprintln!("Send error: {e}"));
}

/// Handle reception of Battery Info and send it to the GUI
async fn handle_battery_info(
    battery_info: &BatteryInfo,
    gui_sender: &mut futures_channel::mpsc::Sender<SubscriptionEvent>,
) {
    gui_sender
        .send(DeviceBatteryLevel(Some(battery_info.level as u32)))
        .await
        .unwrap_or_else(|e| eprintln!("Send error: {e}"));
}

async fn handle_new_channel(
    radio_cache: &mut RadioCache,
    gui_sender: &mut futures_channel::mpsc::Sender<SubscriptionEvent>,
    channel_info: ChannelInfoData,
) {
    radio_cache.known_channels.insert(channel_info.channel_idx);
    gui_sender
        .send(channel_info.into())
        .await
        .unwrap_or_else(|e| eprintln!("Send error: {e}"));
}

async fn handle_neighbours(
    neighbours: NeighboursData,
    gui_sender: &mut futures_channel::mpsc::Sender<SubscriptionEvent>,
) {
    for neighbour in neighbours.neighbours {
        gui_sender
            .send(NewNode(neighbour.into()))
            .await
            .unwrap_or_else(|e| eprintln!("Send error: {e}"));
    }
}

async fn handle_new_channel_message(
    radio_cache: &RadioCache,
    channel_message: ChannelMessage,
    gui_sender: &mut futures_channel::mpsc::Sender<SubscriptionEvent>,
) {
    // extract the node name, look it up and get the node ID or else default to a node id of 0
    let (node_id, text) = if let Some((node_name, text)) = channel_message.text.split_once(": ") {
        (
            match radio_cache.known_contacts.get(node_name) {
                Some(node_id) => *node_id,
                None => NodeId::from(0u64),
            },
            text,
        )
    } else {
        (NodeId::from(0u64), channel_message.text.as_str())
    };

    let mcmessage = MCMessageReceived(
        Channel(ChannelIndex::from(channel_message.channel_idx)),
        channel_message.sender_timestamp.into(),
        node_id,
        MCContent::NewTextMessage(text.to_string()),
        MeshChat::now(),
    );

    gui_sender
        .send(mcmessage)
        .await
        .unwrap_or_else(|e| eprintln!("Send error: {e}"));
}

async fn handle_new_contact_message(
    contact_message: ContactMessage,
    gui_sender: &mut futures_channel::mpsc::Sender<SubscriptionEvent>,
) {
    gui_sender
        .send(contact_message.into())
        .await
        .unwrap_or_else(|e| eprintln!("Send error: {e}"));
}

async fn handle_radio_event(
    radio_cache: &mut RadioCache,
    meshcore: &MeshCore,
    meshcore_event: Box<MeshCoreEvent>,
    gui_sender: &mut futures_channel::mpsc::Sender<SubscriptionEvent>,
) -> meshcore_rs::Result<()> {
    match meshcore_event.event_type {
        EventType::NeighboursResponse => {
            if let EventPayload::Neighbours(neighbours) = meshcore_event.payload {
                handle_neighbours(neighbours, gui_sender).await;
            }
        }
        EventType::Contacts => {
            if let EventPayload::Contacts(contacts) = meshcore_event.payload {
                println!("Contacts: {contacts:?}");
            }
        }
        EventType::NewContact | EventType::NextContact => {
            if let EventPayload::Contact(contact) = meshcore_event.payload {
                gui_sender
                    .send(NewNode(contact.into()))
                    .await
                    .unwrap_or_else(|e| eprintln!("Send error: {e}"));
            }
        }
        EventType::SelfInfo => {
            if let EventPayload::SelfInfo(self_info) = meshcore_event.payload {
                handle_self_info(radio_cache, self_info, gui_sender).await;
            }
        }
        EventType::DeviceInfo => {
            if let EventPayload::DeviceInfo(device_info) = meshcore_event.payload {
                handle_device_info(radio_cache, device_info, gui_sender).await;
            }
        }
        EventType::Battery => {
            if let EventPayload::Battery(battery_info) = meshcore_event.payload {
                handle_battery_info(&battery_info, gui_sender).await;
            }
        }
        EventType::ChannelInfo => {
            if let EventPayload::ChannelInfo(channel_info) = meshcore_event.payload {
                handle_new_channel(radio_cache, gui_sender, channel_info).await;
            }
        }
        EventType::Advertisement => {
            if let EventPayload::Advertisement(advertisement) = meshcore_event.payload {
                gui_sender
                    .send(NewNode((&advertisement).into()))
                    .await
                    .unwrap_or_else(|e| eprintln!("Send error: {e}"));
            }
        }
        EventType::MessagesWaiting => {
            get_pending_messages(radio_cache, meshcore, gui_sender).await;
        }
        EventType::DiscoverResponse => {
            if let EventPayload::DiscoverResponse(discover_entry) = meshcore_event.payload {
                for discovery in discover_entry {
                    gui_sender
                        .send(NewNode(discovery.into()))
                        .await
                        .unwrap_or_else(|e| eprintln!("Send error: {e}"));
                }
            }
        }
        EventType::AdvertResponse => {
            if let EventPayload::AdvertResponse(advert_response) = meshcore_event.payload {
                gui_sender
                    .send(NewNode(advert_response.into()))
                    .await
                    .unwrap_or_else(|e| eprintln!("Send error: {e}"));
            }
        }
        EventType::ContactMsgRecv => {
            if let EventPayload::ContactMessage(contact_message) = meshcore_event.payload {
                handle_new_contact_message(contact_message, gui_sender).await;
            }
        }
        EventType::ChannelMsgRecv => {
            if let EventPayload::ChannelMessage(channel_message) = meshcore_event.payload {
                handle_new_channel_message(radio_cache, channel_message, gui_sender).await;
            }
        }
        EventType::Ack => {
            if let EventPayload::Ack { tag } = meshcore_event.payload {
                let message_id: MessageId = tag.into();
                if let Some(conversation_id) = radio_cache.pending_ack.remove(&message_id) {
                    gui_sender
                        .send(MessageACK(conversation_id, message_id))
                        .await
                        .unwrap_or_else(|e| eprintln!("Send error: {e}"));
                }
            }
        }
        EventType::NoMoreMessages | EventType::Ok => {}
        EventType::Error => {
            // wait for a response, but here we will receive a duplicate....
            if let EventPayload::String(message) = meshcore_event.payload {
                gui_sender
                    .send(ConnectionError(
                        radio_cache.self_id.to_string(),
                        "Radio Error".to_string(),
                        message,
                    ))
                    .await
                    .unwrap_or_else(|e| eprintln!("Send error: {e}"));
            }
        }
        EventType::LogData => { /* LogData payload */ }
        EventType::MsgSent => { /* Doesn't have the message body to reflect back to GUI */ }
        _ => {
            println!(
                "Unhandled Event Type ({}) = {meshcore_event:?}",
                meshcore_event.event_type as u32
            );
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::device::SubscriptionEvent;
    use futures::StreamExt;
    use futures::channel::mpsc;
    use meshcore_rs::events::{BatteryInfo, SelfInfo};

    // Helper to create a test sender/receiver pair
    fn create_test_channel() -> (
        mpsc::Sender<SubscriptionEvent>,
        mpsc::Receiver<SubscriptionEvent>,
    ) {
        mpsc::channel(100)
    }

    // Helper to create SelfInfo for testing
    fn create_test_self_info(name: &str, public_key: [u8; 32], lat: i32, lon: i32) -> SelfInfo {
        SelfInfo {
            adv_type: 0,
            tx_power: 20,
            max_tx_power: 30,
            public_key,
            adv_lat: lat,
            adv_lon: lon,
            multi_acks: 0,
            adv_loc_policy: 0,
            telemetry_mode_base: 0,
            telemetry_mode_loc: 0,
            telemetry_mode_env: 0,
            manual_add_contacts: false,
            radio_freq: 915_000_000,
            radio_bw: 250_000,
            sf: 12,
            cr: 5,
            name: name.to_string(),
        }
    }

    // Tests for RadioCache

    #[test]
    fn radio_cache_default() {
        let cache = RadioCache::default();
        assert_eq!(cache.self_id, NodeId::from(0u64));
        assert!(cache.known_channels.is_empty());
    }

    #[test]
    fn radio_cache_known_channels() {
        let mut cache = RadioCache::default();
        assert!(!cache.known_channels.contains(&0));

        cache.known_channels.insert(0);
        assert!(cache.known_channels.contains(&0));

        cache.known_channels.insert(1);
        cache.known_channels.insert(2);
        assert_eq!(cache.known_channels.len(), 3);
    }

    #[test]
    fn radio_cache_self_id() {
        let cache = RadioCache {
            self_id: NodeId::from(0x0102_0304_0506_0708u64),
            ..Default::default()
        };
        assert_eq!(cache.self_id, NodeId::from(0x0102_0304_0506_0708u64));
    }

    // Tests for handle_self_info

    #[tokio::test]
    async fn handle_self_info_sends_events() {
        let mut public_key = [0u8; 32];
        public_key[0..8].copy_from_slice(&[0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08]);
        let self_info = create_test_self_info("TestNode", public_key, 37_774_900, -122_419_400);

        let mut radio_cache = RadioCache::default();
        let (mut sender, mut receiver) = create_test_channel();

        handle_self_info(&mut radio_cache, self_info, &mut sender).await;

        // Check radio_cache was updated
        assert_eq!(radio_cache.self_id, NodeId::from(0x0102_0304_0506_0708u64));

        // Check MyNodeNum event was sent
        let event1 = receiver.next().await.expect("Expected MyNodeNum event");
        let MyNodeNum(node_id) = event1 else {
            unreachable!("Expected MyNodeNum event")
        };
        assert_eq!(node_id, NodeId::from(0x0102_0304_0506_0708u64));

        // Check MyUserInfo event was sent
        let event2 = receiver.next().await.expect("Expected MyUserInfo event");
        let MyUserInfo(user) = event2 else {
            unreachable!("Expected MyUserInfo event")
        };
        assert_eq!(user.long_name, "TestNode");

        // Check MyPosition event was sent
        let event3 = receiver.next().await.expect("Expected MyPosition event");
        let MyPosition(position) = event3 else {
            unreachable!("Expected MyPosition event")
        };
        assert!((position.latitude - 37.7749).abs() < 0.0001);
        assert!((position.longitude - -122.4194).abs() < 0.0001);
    }

    #[tokio::test]
    async fn handle_self_info_empty_name() {
        let public_key = [0u8; 32];
        let self_info = create_test_self_info("", public_key, 0, 0);

        let mut radio_cache = RadioCache::default();
        let (mut sender, mut receiver) = create_test_channel();

        handle_self_info(&mut radio_cache, self_info, &mut sender).await;

        // Skip MyNodeNum
        let _ = receiver.next().await;

        // Check MyUserInfo with empty name
        let event = receiver.next().await.expect("Expected MyUserInfo event");
        let MyUserInfo(user) = event else {
            unreachable!("Expected MyUserInfo event")
        };
        assert_eq!(user.long_name, "");
    }

    // Tests for handle_battery_info

    #[tokio::test]
    async fn handle_battery_info_sends_event() {
        let battery_info = BatteryInfo {
            level: 75,
            storage: 1000,
        };

        let (mut sender, mut receiver) = create_test_channel();

        handle_battery_info(&battery_info, &mut sender).await;

        let event = receiver
            .next()
            .await
            .expect("Expected DeviceBatteryLevel event");
        let DeviceBatteryLevel(level) = event else {
            unreachable!("Expected DeviceBatteryLevel event")
        };
        assert_eq!(level, Some(75));
    }

    #[tokio::test]
    async fn handle_battery_info_zero_level() {
        let battery_info = BatteryInfo {
            level: 0,
            storage: 0,
        };

        let (mut sender, mut receiver) = create_test_channel();

        handle_battery_info(&battery_info, &mut sender).await;

        let event = receiver
            .next()
            .await
            .expect("Expected DeviceBatteryLevel event");
        let DeviceBatteryLevel(level) = event else {
            unreachable!("Expected DeviceBatteryLevel event")
        };
        assert_eq!(level, Some(0));
    }

    #[tokio::test]
    async fn handle_battery_info_full() {
        let battery_info = BatteryInfo {
            level: 100,
            storage: 5000,
        };

        let (mut sender, mut receiver) = create_test_channel();

        handle_battery_info(&battery_info, &mut sender).await;

        let event = receiver
            .next()
            .await
            .expect("Expected DeviceBatteryLevel event");
        let DeviceBatteryLevel(level) = event else {
            unreachable!("Expected DeviceBatteryLevel event")
        };
        assert_eq!(level, Some(100));
    }

    // Additional edge case tests for handle_self_info

    #[tokio::test]
    async fn handle_self_info_with_unicode_name() {
        let public_key = [0xAA; 32];
        let self_info =
            create_test_self_info("日本語ノード🎉", public_key, 35_681_400, 139_767_100);

        let mut radio_cache = RadioCache::default();
        let (mut sender, mut receiver) = create_test_channel();

        handle_self_info(&mut radio_cache, self_info, &mut sender).await;

        // Skip MyNodeNum
        let _ = receiver.next().await;

        // Check MyUserInfo with unicode name
        let event = receiver.next().await.expect("Expected MyUserInfo event");
        let MyUserInfo(user) = event else {
            unreachable!("Expected MyUserInfo event")
        };
        assert_eq!(user.long_name, "日本語ノード🎉");
    }

    #[tokio::test]
    async fn handle_self_info_extreme_coordinates() {
        let public_key = [0u8; 32];
        // Near the North Pole
        let self_info = create_test_self_info("ArcticNode", public_key, 89_999_000, 0);

        let mut radio_cache = RadioCache::default();
        let (mut sender, mut receiver) = create_test_channel();

        handle_self_info(&mut radio_cache, self_info, &mut sender).await;

        // Skip MyNodeNum and MyUserInfo
        let _ = receiver.next().await;
        let _ = receiver.next().await;

        // Check MyPosition with extreme coordinates
        let event = receiver.next().await.expect("Expected MyPosition event");
        let MyPosition(position) = event else {
            unreachable!("Expected MyPosition event")
        };
        assert!((position.latitude - 89.999).abs() < 0.001);
    }

    #[tokio::test]
    async fn handle_self_info_negative_coordinates() {
        let public_key = [0u8; 32];
        // Sydney, Australia
        let self_info = create_test_self_info("SydneyNode", public_key, -33_868_800, 151_209_300);

        let mut radio_cache = RadioCache::default();
        let (mut sender, mut receiver) = create_test_channel();

        handle_self_info(&mut radio_cache, self_info, &mut sender).await;

        // Skip MyNodeNum and MyUserInfo
        let _ = receiver.next().await;
        let _ = receiver.next().await;

        let event = receiver.next().await.expect("Expected MyPosition event");
        let MyPosition(position) = event else {
            unreachable!("Expected MyPosition event")
        };
        assert!((position.latitude - -33.8688).abs() < 0.0001);
        assert!((position.longitude - 151.2093).abs() < 0.0001);
    }

    // Test RadioCache updates self_id correctly from different public keys

    #[tokio::test]
    async fn handle_self_info_updates_radio_cache_self_id() {
        let mut public_key = [0u8; 32];
        public_key[0..8].copy_from_slice(&[0xDE, 0xAD, 0xBE, 0xEF, 0xCA, 0xFE, 0xBA, 0xBE]);
        let self_info = create_test_self_info("CafeNode", public_key, 0, 0);

        let mut radio_cache = RadioCache::default();
        assert_eq!(radio_cache.self_id, NodeId::from(0u64));

        let (mut sender, _receiver) = create_test_channel();
        handle_self_info(&mut radio_cache, self_info, &mut sender).await;

        assert_eq!(radio_cache.self_id, NodeId::from(0xDEAD_BEEF_CAFE_BABEu64));
    }

    // Test battery info with max u16 value

    #[tokio::test]
    async fn handle_battery_info_max_level() {
        let battery_info = BatteryInfo {
            level: u16::MAX,
            storage: u16::MAX,
        };

        let (mut sender, mut receiver) = create_test_channel();

        handle_battery_info(&battery_info, &mut sender).await;

        let event = receiver
            .next()
            .await
            .expect("Expected DeviceBatteryLevel event");
        let DeviceBatteryLevel(level) = event else {
            unreachable!("Expected DeviceBatteryLevel event")
        };
        assert_eq!(level, Some(u16::MAX as u32));
    }

    // Test that multiple events can be sent through the channel

    #[tokio::test]
    async fn multiple_battery_info_events() {
        let (mut sender, mut receiver) = create_test_channel();

        // Send multiple battery updates
        handle_battery_info(
            &BatteryInfo {
                level: 100,
                storage: 0,
            },
            &mut sender,
        )
        .await;
        handle_battery_info(
            &BatteryInfo {
                level: 75,
                storage: 0,
            },
            &mut sender,
        )
        .await;
        handle_battery_info(
            &BatteryInfo {
                level: 50,
                storage: 0,
            },
            &mut sender,
        )
        .await;

        // Receive all three
        let DeviceBatteryLevel(level1) = receiver.next().await.expect("event 1") else {
            unreachable!("Expected DeviceBatteryLevel")
        };
        let DeviceBatteryLevel(level2) = receiver.next().await.expect("event 2") else {
            unreachable!("Expected DeviceBatteryLevel")
        };
        let DeviceBatteryLevel(level3) = receiver.next().await.expect("event 3") else {
            unreachable!("Expected DeviceBatteryLevel")
        };

        assert_eq!(level1, Some(100));
        assert_eq!(level2, Some(75));
        assert_eq!(level3, Some(50));
    }

    #[test]
    fn self_info_to_mcuser() {
        let mut public_key = [0u8; 32];
        public_key[0..8].copy_from_slice(&[0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08]);
        let self_info = create_test_self_info("MyNode", public_key, 0, 0);

        let radio_cache = RadioCache {
            self_info: self_info.clone(),
            ..Default::default()
        };
        let user: MCUser = radio_cache.user();

        assert_eq!(user.long_name, "MyNode");
        assert_eq!(user.short_name, "MyNode");
        assert_eq!(user.id, 0x0102_0304_0506_0708_u64.to_string());
    }

    #[test]
    fn self_info_to_mcuser_empty_name() {
        let public_key = [0u8; 32];
        let self_info = create_test_self_info("", public_key, 0, 0);

        let radio_cache = RadioCache {
            self_info: self_info.clone(),
            ..Default::default()
        };
        let user: MCUser = radio_cache.user();

        assert_eq!(user.long_name, "");
        assert_eq!(user.short_name, "");
    }

    #[test]
    fn self_info_to_mcuser_unicode_name() {
        let public_key = [0xFF; 32];
        let self_info = create_test_self_info("日本語ノード", public_key, 0, 0);

        let radio_cache = RadioCache {
            self_info: self_info.clone(),
            ..Default::default()
        };
        let user: MCUser = radio_cache.user();

        assert_eq!(user.long_name, "日本語ノード");
        assert_eq!(user.short_name, "日本語ノード");
    }

    // Tests for local timestamp usage

    #[tokio::test]
    async fn handle_new_channel_message_uses_local_timestamp() {
        use meshcore_rs::ChannelMessage;

        let radio_cache = RadioCache::default();
        let (mut sender, mut receiver) = create_test_channel();

        let channel_message = ChannelMessage {
            channel_idx: 0,
            sender_timestamp: 1234567890, // Old radio timestamp (should be ignored)
            text: "Test message".to_string(),
            path_len: 0,
            txt_type: 0,
            snr: None,
        };

        let before = MeshChat::now();
        handle_new_channel_message(&radio_cache, channel_message, &mut sender).await;
        let after = MeshChat::now();

        let event = receiver
            .next()
            .await
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
    async fn handle_new_contact_message_uses_local_timestamp() {
        use meshcore_rs::ContactMessage;

        let (mut sender, mut receiver) = create_test_channel();

        let contact_message = ContactMessage {
            sender_prefix: [0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF],
            path_len: 0,
            txt_type: 0,
            sender_timestamp: 1234567890, // Old radio timestamp (should be ignored)
            text: "Direct message".to_string(),
            snr: Some(10.5),
            signature: None,
        };

        let before = MeshChat::now();
        handle_new_contact_message(contact_message, &mut sender).await;
        let after = MeshChat::now();

        let event = receiver
            .next()
            .await
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
}
