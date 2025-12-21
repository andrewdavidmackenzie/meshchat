use crate::Message;
use crate::Message::{CopyToClipBoard, DeviceViewEvent, ShowLocation};
use crate::channel_view::ChannelViewMessage::MessageSeen;
use crate::channel_view::{ChannelId, ChannelViewMessage};
use crate::channel_view_entry::Payload::{
    AlertMessage, EmojiReply, NewTextMessage, PositionMessage, TextMessageReply, UserMessage,
};
use crate::device_view::DeviceViewMessage::{ChannelMsg, ShowChannel, StartForwardingMessage};
use crate::device_view::short_name;
use crate::styles::{
    COLOR_DICTIONARY, COLOR_GREEN, MY_MESSAGE_BUBBLE_STYLE, OTHERS_MESSAGE_BUBBLE_STYLE,
    TIME_TEXT_COLOR, TIME_TEXT_SIZE, TIME_TEXT_WIDTH, alert_message_style, button_chip_style,
    menu_button_style, message_text_style,
};
use chrono::{DateTime, Local, Utc};
use iced::Length::Fixed;
use iced::font::Weight;
use iced::widget::{Column, Container, Row, Space, Text, button, sensor, text, tooltip};
use iced::{Bottom, Color, Element, Fill, Font, Left, Padding, Renderer, Right, Theme, Top};
use iced_aw::menu::Menu;
use iced_aw::{MenuBar, menu_bar, menu_items};
use meshtastic::protobufs::{NodeInfo, User};
use ringmap::RingMap;
use std::cmp::Ordering;
use std::collections::HashMap;
use std::default::Default;
use std::fmt;
use std::fmt::Formatter;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Clone, Debug)]
pub enum Payload {
    AlertMessage(String),
    NewTextMessage(String),
    /// TextMessageReply(reply_to_id, reply text)
    TextMessageReply(u32, String),
    /// EmojiReply(reply_to_id, emoji_code string)
    EmojiReply(u32, String),
    PositionMessage(i32, i32),
    UserMessage(User),
}

impl fmt::Display for Payload {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            AlertMessage(text_message) => f.write_str(text_message),
            NewTextMessage(text_message) => f.write_str(text_message),
            TextMessageReply(_, reply_text) => f.write_str(reply_text),
            EmojiReply(_, _) => f.write_str(""), // Not possible
            PositionMessage(lat, long) => f.write_str(&ChannelViewEntry::location_text(lat, long)),
            UserMessage(user) => f.write_str(&ChannelViewEntry::user_text(user)),
        }
    }
}

impl Default for Payload {
    fn default() -> Self {
        NewTextMessage(String::default())
    }
}

/// An entry in the Channel View that represents some type of message sent to either this user on
/// this device or to a channel this device can read. Can be any of [Payload] types.
#[allow(dead_code)] // Remove when the 'seen' field is used
#[derive(Clone, Debug, Default)]
pub struct ChannelViewEntry {
    /// NodeId of the node that sent this message
    from: u32,
    // Unique message ID for this message
    message_id: u32,
    /// The daytime the message was sent/received
    rx_daytime: DateTime<Local>,
    /// The message contents of differing types
    payload: Payload,
    /// Has the user of the app seen this message?
    pub seen: bool,
    /// Has the entry been acknowledged as received by a receiver?
    acked: bool,
    /// Map of emojis and for each emoji there is the string for it and a number of node ids
    /// who sent that emoji
    emoji_reply: HashMap<String, Vec<u32>>,
}

impl ChannelViewEntry {
    /// Create a new [ChannelViewEntry] from the parameters provided. The received time will be set to
    /// the current time in EPOC as an u64
    pub fn new(payload: Payload, from: u32, message_id: u32) -> Self {
        ChannelViewEntry {
            payload,
            from,
            message_id,
            rx_daytime: Self::now(),
            ..Default::default()
        }
    }

    /// Get the time now as a [DateTime<Local>]
    fn now() -> DateTime<Local> {
        let rx_time = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|t| t.as_secs())
            .unwrap_or(0);

        let datetime_utc = DateTime::<Utc>::from_timestamp_secs(rx_time as i64).unwrap();
        datetime_utc.with_timezone(&Local)
    }

    /// Return the node id that sent the message
    pub fn from(&self) -> u32 {
        self.from
    }

    /// Get a reference to the payload of this message
    pub fn payload(&self) -> &Payload {
        &self.payload
    }

    /// Return the message_id
    pub fn message_id(&self) -> u32 {
        self.message_id
    }

    /// Mark the Entry as acknowledgeMd
    pub fn ack(&mut self) {
        self.acked = true;
    }

    /// Add an emoji reply to this entry
    pub fn add_emoji(&mut self, emoji_string: String, from: u32) {
        self.emoji_reply
            .entry(emoji_string)
            .and_modify(|sender_vec| sender_vec.push(from))
            .or_insert(vec![from]);
    }

    /// Return true if the radio has acknowledged this message
    pub fn acked(&self) -> bool {
        self.acked
    }

    /// Return the emoji reply to this message, if any.
    pub fn emojis(&self) -> &HashMap<String, Vec<u32>> {
        &self.emoji_reply
    }

    /// Return the time this message was received/sent as u64 seconds in EPOCH time
    pub fn time(&self) -> DateTime<Local> {
        self.rx_daytime
    }

    /// Order two messages - using the rx_daytime field.
    pub fn sort_by_rx_time(_: &u32, left: &Self, _: &u32, right: &Self) -> Ordering {
        left.rx_daytime.cmp(&right.rx_daytime)
    }

    /// Return the text to use when replying to this message
    pub fn reply_quote(entries: &RingMap<u32, ChannelViewEntry>, id: &u32) -> Option<String> {
        entries
            .get(id)
            .map(|entry| format!("Re: {}", entry.payload))
            .map(|mut text| {
                if text.len() > 20 {
                    text = Self::truncate(&text, 20).to_string();
                    format!("{} ...", text)
                } else {
                    text
                }
            })
    }

    /// Truncate a String at a character boundary
    fn truncate(s: &str, max_chars: usize) -> &str {
        match s.char_indices().nth(max_chars) {
            None => s,
            Some((idx, _)) => &s[..idx],
        }
    }

    /// Hash the node name into a color index - to be able to assign a consistent color to the text
    /// name for a node in the UI
    fn color_from_id(name: u32) -> Color {
        let mut hasher = DefaultHasher::new();
        name.hash(&mut hasher);
        let hash = hasher.finish();

        let index = (hash % COLOR_DICTIONARY.len() as u64) as usize;
        COLOR_DICTIONARY[index]
    }

    /// Format a time as seconds in epoc (u64) into a String of hour and minutes during the day
    /// it occurs in. These will be separated by Day specifiers, so day is not needed.
    fn time_to_text(datetime_local: DateTime<Local>) -> Text<'static, Theme, Renderer> {
        let time_str = datetime_local.format("%H:%M").to_string(); // Formats as HH:MM
        text(time_str)
            .color(TIME_TEXT_COLOR)
            .size(TIME_TEXT_SIZE)
            .width(Fixed(TIME_TEXT_WIDTH))
    }

    /// Return an element (currently a Column) with a list of the names of the nodes that sent the
    /// given emoji.
    fn list_of_nodes<'a>(
        nodes: &'a HashMap<u32, NodeInfo>,
        sources: &'a Vec<u32>,
    ) -> Element<'a, Message> {
        let mut col = Column::new();
        for source in sources {
            col = col.push(text(short_name(nodes, *source)).color(Self::color_from_id(*source)));
        }
        col.into()
    }

    /// Create an Element that contains a message received or sent
    pub fn view<'a>(
        &'a self,
        entries: &'a RingMap<u32, ChannelViewEntry>,
        nodes: &'a HashMap<u32, NodeInfo>,
        channel_id: &'a ChannelId,
        mine: bool,
    ) -> Element<'a, Message> {
        let name = short_name(nodes, self.from);

        let message_text = match self.payload() {
            AlertMessage(text_msg) => text_msg.clone(),
            NewTextMessage(text_msg) => text_msg.clone(),
            TextMessageReply(_, text_msg) => text_msg.clone(),
            PositionMessage(lat, long) => Self::location_text(lat, long),
            UserMessage(user) => Self::user_text(user),
            EmojiReply(_, _) => String::default(), // Should never happen
        };

        let mut message_content_column = Column::new();

        if !mine {
            message_content_column =
                self.top_row(message_content_column, name, message_text.clone());
        }

        let content: Element<'static, Message> = match self.payload() {
            AlertMessage(_) => text(message_text)
                .style(alert_message_style)
                .size(18)
                .into(),
            NewTextMessage(_) | UserMessage(_) => {
                text(message_text).style(message_text_style).size(18).into()
            }
            TextMessageReply(reply_to_id, _) => {
                if let Some(reply_quote) = Self::reply_quote(entries, reply_to_id) {
                    let quote_row = Row::new().push(text(reply_quote).color(COLOR_GREEN));
                    message_content_column = message_content_column.push(quote_row);
                };
                text(message_text).style(message_text_style).size(18).into()
            }
            PositionMessage(lat, long) => button(text(message_text))
                .padding([1, 5])
                .style(button_chip_style)
                .on_press(ShowLocation(*lat, *long))
                .into(),
            EmojiReply(_, _) => text(message_text).into(),
        };

        // Create the row with message text and time and maybe an ACK tick mark
        let mut text_and_time_row = Row::new()
            .push(content)
            .push(Space::new().width(10.0))
            .push(Self::time_to_text(self.time()))
            .align_y(Bottom);

        if self.acked() {
            text_and_time_row = text_and_time_row.push(text("✓").size(14).color(COLOR_GREEN))
        };

        // Add the message text and time row
        message_content_column = message_content_column.push(text_and_time_row);

        let style = if mine {
            MY_MESSAGE_BUBBLE_STYLE
        } else {
            OTHERS_MESSAGE_BUBBLE_STYLE
        };

        let mut message_row = Row::new().padding([6, 6]);
        if !self.emojis().is_empty() {
            // Butt the emoji_row up against the bubble
            message_row = message_row.padding(Padding {
                top: 6.0,
                right: 6.0,
                bottom: 0.0,
                left: 6.0,
            });
        }

        // The outer container object that spans the width of the view and justifies the message
        // row within it depending on who sent the message
        let mut message_column = Column::new().width(Fill);

        let message_bubble = Container::new(message_content_column)
            .padding([6, 8])
            .style(move |_theme: &Theme| style);

        // Put on the right-hand side if my message, on the left if from someone else
        if mine {
            // Avoid very wide messages from me extending all the way to the left edge of the screen
            message_row = message_row
                .push(Space::new().width(100.0))
                .push(message_bubble);
            message_column = message_column.align_x(Right).push(message_row);
        } else {
            // Avoid very wide messages from others extending all the way to the right edge
            message_row = message_row
                .push(message_bubble)
                .push(Space::new().width(100.0));
            message_column = message_column.align_x(Left).push(message_row);
        };

        // Add the emoji row outside the bubble, below it
        message_column = self.emoji_row(nodes, message_column);

        sensor(message_column)
            .on_show(|_| {
                DeviceViewEvent(ChannelMsg(MessageSeen(channel_id.clone(), self.message_id)))
            })
            .into()
    }

    fn user_text(user: &User) -> String {
        format!(
            "ⓘ from '{}' ('{}'), id = '{}', with hardware '{}'",
            user.long_name,
            user.short_name,
            user.id,
            user.hw_model().as_str_name()
        )
    }

    fn location_text(lat: &i32, long: &i32) -> String {
        let latitude = 0.0000001 * *lat as f64;
        let longitude = 0.0000001 * *long as f64;
        format!("📌 {:.2}, {:.2}", latitude, longitude)
    }

    /// Add a row to the content column with the name of the source node
    fn top_row<'a>(
        &'a self,
        message_content_column: Column<'a, Message>,
        name: &'a str,
        message: String,
    ) -> Column<'a, Message> {
        let text_color = Self::color_from_id(self.from);
        let mut top_row = Row::new().padding(0).align_y(Top);

        top_row = top_row
            .push(self.menu_bar(name, message))
            .push(Space::new().width(2.0));

        top_row = top_row.push(
            text(name)
                .font(Font {
                    weight: Weight::Bold,
                    ..Default::default()
                })
                .color(text_color),
        );

        message_content_column.push(top_row)
    }

    /// Append an element to the column that contains the emoji replies for this message, if any.
    fn emoji_row<'a>(
        &'a self,
        nodes: &'a HashMap<u32, NodeInfo>,
        mut message_column: Column<'a, Message>,
    ) -> Column<'a, Message> {
        if !self.emojis().is_empty() {
            let mut emoji_row = Row::new().padding(Padding {
                top: -4.0,
                right: 6.0,
                bottom: 6.0,
                left: 6.0,
            });
            for (emoji, sources) in self.emojis() {
                let tooltip_element: Element<'_, Message> = Self::list_of_nodes(nodes, sources);
                emoji_row = emoji_row.push(
                    tooltip(
                        text(emoji.clone()).size(18).align_y(Top),
                        tooltip_element,
                        tooltip::Position::Bottom,
                    )
                    .padding(0),
                );
            }

            message_column = message_column.push(emoji_row);
        }

        message_column
    }

    // TODO differentiate if we are in a node or channel view
    // if a node view, don't show the DM menu item
    fn menu_bar<'a>(
        &self,
        name: &'a str,
        message: String,
    ) -> MenuBar<'a, Message, Theme, Renderer> {
        let menu_tpl_1 = |items| Menu::new(items).spacing(3);

        let dm = format!("DM with {}", name);
        //(menu_button("react".into(), Message::None))
        #[rustfmt::skip]
        let menu_items = menu_items!(
            (menu_button("copy".into(), CopyToClipBoard(message.to_string()))),
            (menu_button("forward".into(), DeviceViewEvent(StartForwardingMessage(self.clone())))),
            (menu_button("reply".into(), DeviceViewEvent(ChannelMsg(ChannelViewMessage::PrepareReply(self.message_id))))),
            (menu_button(dm, DeviceViewEvent(ShowChannel(Some(ChannelId::Node(self.from()))))))
        );

        // Create the menu bar with the root button and list of options
        menu_bar!((menu_root_button("▼"), {
            menu_tpl_1(menu_items).width(140)
        }))
        .close_on_background_click(true)
        .close_on_item_click(true)
        .style(menu_button_style)
    }
}

fn menu_button(
    label: String,
    message: Message,
) -> button::Button<'static, Message, Theme, Renderer> {
    button(text(label))
        .padding([4, 8])
        .style(button_chip_style)
        .on_press(message)
        .width(Fill)
}

fn menu_root_button(label: &str) -> button::Button<'_, Message, Theme, Renderer> {
    button(text(label).size(14))
        .padding([0, 4])
        .style(button_chip_style)
        .on_press(Message::None) // Needed for styling to work
}

#[cfg(test)]
impl PartialEq<Self> for ChannelViewEntry {
    fn eq(&self, other: &Self) -> bool {
        self.rx_daytime == other.rx_daytime
    }
}
