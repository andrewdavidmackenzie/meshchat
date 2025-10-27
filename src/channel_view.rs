use crate::channel_view::ChannelId::Channel;
use crate::channel_view::ChannelViewMessage::{ClearMessage, MessageInput};
use crate::device_view::DeviceViewMessage::{ChannelMsg, SendMessage};
use crate::styles::{text_input_style, NO_SHADOW, RADIUS_12};
use crate::{channel_message::ChannelMessage, Message};
use iced::widget::container::Style;
use iced::widget::scrollable::Scrollbar;
use iced::widget::{scrollable, text, text_input, Column, Container, Row, Space};
use iced::{Background, Border, Color, Element, Fill, Left, Right, Task, Theme};
use meshtastic::protobufs::mesh_packet::PayloadVariant::Decoded;
use meshtastic::protobufs::{MeshPacket, PortNum};
use meshtastic::Message as _;
use serde::{Deserialize, Serialize};
use sorted_vec::SortedVec;
use std::fmt::{Display, Formatter};
use std::time::{SystemTime, UNIX_EPOCH};

const MESSAGE_BORDER: Border = Border {
    radius: RADIUS_12, // rounded corners
    width: 2.0,
    color: Color::WHITE,
};

const MY_STYLE: Style = Style {
    text_color: Some(Color::WHITE),
    background: Some(Background::Color(Color::from_rgba(0.08, 0.3, 0.22, 1.0))),
    border: MESSAGE_BORDER,
    shadow: NO_SHADOW,
};

const OTHERS_STYLE: Style = Style {
    text_color: Some(Color::WHITE),
    background: Some(Background::Color(Color::from_rgba(0.0, 0.0, 0.0, 1.0))),
    border: MESSAGE_BORDER,
    shadow: NO_SHADOW,
};

#[derive(Debug, Clone)]
pub enum ChannelViewMessage {
    MessageInput(String),
    ClearMessage,
}

#[derive(Debug, Clone, Serialize, Deserialize, Hash, PartialEq, Eq)]
pub enum ChannelId {
    Channel(i32), // Channel::index 0..7
    User(String), // User::id
}

impl Default for ChannelId {
    fn default() -> Self {
        Channel(0)
    }
}

impl Display for ChannelId {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "{:?}", self)
    }
}

pub struct ChannelView {
    pub(crate) channel_id: ChannelId,
    pub packets: Vec<MeshPacket>,
    message: String,                     // Message typed in so far
    messages: SortedVec<ChannelMessage>, // Messages received so far
    my_source: u32,
}

// A view of a single channel and it's message, which maybe a real radio "Channel" or a chat channel
// with a specific [meshtastic:User]
impl ChannelView {
    pub fn new(channel_id: ChannelId, source: u32) -> Self {
        Self {
            channel_id,
            packets: Vec::new(),
            message: String::new(),
            messages: SortedVec::new(),
            my_source: source,
        }
    }

    pub fn message_sent(&mut self) {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|t| t.as_secs())
            .unwrap_or(0);

        // TODO Mark as sent in the UI, and clear the message entry
        self.messages.push(ChannelMessage {
            text: self.message.clone(),
            from: self.my_source,
            rx_time: now, // time in epoc
        });
        // Until we have a queue of messages being sent pending confirmation
        self.message = String::new();
    }

    pub fn push_packet(&mut self, mesh_packet: MeshPacket) {
        if let Some(Decoded(data)) = &mesh_packet.payload_variant
            && data.emoji == 0
        // TODO handle emoji replies
        {
            match PortNum::try_from(data.portnum) {
                Ok(PortNum::TextMessageApp) => {
                    let now = SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .map(|t| t.as_secs())
                        .unwrap_or(0);

                    self.messages.push(ChannelMessage {
                        text: String::from_utf8(data.payload.clone()).unwrap(),
                        from: mesh_packet.from,
                        rx_time: now,
                    });
                }
                Ok(PortNum::PositionApp) => println!("Position payload"),
                Ok(PortNum::AlertApp) => println!("Alert payload"),
                Ok(PortNum::TelemetryApp) => println!("Telemetry payload"),
                Ok(PortNum::NeighborinfoApp) => println!("Neighbor Info payload"),
                // TODO will need to parse a lot of these at the next layer up before here
                Ok(PortNum::NodeinfoApp) => {
                    let buf = &data.payload as &[u8];
                    let user = meshtastic::protobufs::User::decode(buf).unwrap();
                    println!("Ping from User: {}", user.short_name);
                }
                _ => eprintln!("Unexpected payload type from radio: {}", data.portnum),
            }
        }
    }

    pub fn num_packets(&self) -> usize {
        self.packets.len()
    }

    pub fn update(&mut self, channel_view_message: ChannelViewMessage) -> Task<Message> {
        match channel_view_message {
            MessageInput(s) => {
                self.message = s;
                Task::none()
            }
            ClearMessage => {
                self.message = String::new();
                Task::none()
            }
        }
    }

    // Make this a struct and move the message field in here
    pub fn view(&self) -> Element<'static, Message> {
        let mut channel_view = Column::new();

        for message in &self.messages {
            channel_view = channel_view.push(message_box(
                message.text.clone(),
                message.from == self.my_source,
            ));
        }

        let channel_scroll = scrollable(channel_view)
            .direction({
                let scrollbar = Scrollbar::new().width(10.0);
                scrollable::Direction::Vertical(scrollbar)
            })
            .width(Fill)
            .height(Fill);

        Column::new()
            .padding(4)
            .push(channel_scroll)
            .push(self.input_box())
            .into()
    }

    // TODO set an icon,
    // TODO Add to messages in the channel for display, or wait for packet back from radio
    // as a confirmation? Maybe add as sending status?
    // Display it just above the text input until confirmed by arriving in channel?
    // for now only sent to the subscription
    // TODO add an id to the message, or get it back from the subscription to be
    // able to handle replies to it later. Get a timestamp and maybe sender id
    // when TextSent then add to the UI list of messages, interleaved with
    // those received using the timestamp
    // TODO add an icon for sending a message
    // TODO add a method (button?) to clear the text and maybe keyboard short cuts

    fn input_box(&self) -> Element<'static, Message> {
        // TODO move styles to constants

        text_input("Send Message", &self.message)
            .style(text_input_style)
            .on_input(|s| Message::Device(ChannelMsg(MessageInput(s))))
            .on_submit(Message::Device(SendMessage(
                self.message.clone(),
                self.channel_id.clone(),
            )))
            .padding([6, 6])
            .into()
    }
}

fn message_box(msg: String, me: bool) -> Element<'static, Message> {
    let style = if me { MY_STYLE } else { OTHERS_STYLE };

    let bubble = Container::new(
        iced::widget::text(msg)
            .align_x(Right)
            .shaping(text::Shaping::Advanced),
    )
    .padding([6, 12])
    .style(move |_theme: &Theme| style);

    let mut row = Row::new().padding([6, 6]);
    if me {
        row = row.push(Space::new(100.0, 1.0)).push(bubble);
        let col = Column::new().width(Fill).align_x(Right);
        col.push(row).into()
    } else {
        row = row.push(bubble).push(Space::new(100.0, 1.0));
        let col = Column::new().width(Fill).align_x(Left);
        col.push(row).into()
    }
}
