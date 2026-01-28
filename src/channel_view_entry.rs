use crate::Message::{CopyToClipBoard, DeviceViewEvent, OpenUrl, ShowLocation};
use crate::channel_id::ChannelId;
use crate::channel_view::ChannelViewMessage;
use crate::channel_view::ChannelViewMessage::{MessageSeen, MessageUnseen, ReplyWithEmoji};
use crate::channel_view_entry::MCMessage::{
    AlertMessage, EmojiReply, NewTextMessage, PositionMessage, TextMessageReply, UserMessage,
};
use crate::device_view::DeviceViewMessage::{ChannelMsg, ShowChannel, StartForwardingMessage};
use crate::device_view::{long_name, short_name};
use crate::styles::{
    COLOR_DICTIONARY, COLOR_GREEN, MY_MESSAGE_BUBBLE_STYLE, OTHERS_MESSAGE_BUBBLE_STYLE,
    TIME_TEXT_COLOR, TIME_TEXT_SIZE, TIME_TEXT_WIDTH, alert_message_style, button_chip_style,
    menu_button_style, message_text_style, tooltip_style,
};
use crate::{MCNodeInfo, MCPosition, MCUser, Message};
use chrono::{DateTime, Local, Utc};
use iced::Length::Fixed;
use iced::advanced::text::Span;
use iced::font::Weight;
use iced::widget::{
    Column, Container, Row, Space, Text, Tooltip, button, rich_text, sensor, span, text, tooltip,
};
use iced::{Bottom, Color, Element, Fill, Font, Left, Padding, Renderer, Right, Theme, Top};
use iced_aw::menu::Menu;
use iced_aw::{MenuBar, menu_bar, menu_items};
use ringmap::RingMap;
use std::cmp::Ordering;
use std::collections::HashMap;
use std::default::Default;
use std::fmt;
use std::fmt::Formatter;
use std::hash::{DefaultHasher, Hash, Hasher};

#[derive(Clone, Debug)]
pub enum MCMessage {
    AlertMessage(String),          // message_text
    NewTextMessage(String),        // message_text
    TextMessageReply(u32, String), // reply_id, reply_text
    EmojiReply(u32, String),       // reply_id, reply_emoji_text
    PositionMessage(MCPosition),   // position
    UserMessage(MCUser),           // user
}

impl fmt::Display for MCMessage {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            AlertMessage(text) => f.write_str(text.as_str()),
            NewTextMessage(text) => f.write_str(text.as_str()),
            TextMessageReply(_reply_id, text) => f.write_str(format!("Re: {}", text).as_str()),
            EmojiReply(reply_id, text) => f.write_str(format!("{}: {}", text, reply_id).as_str()),
            PositionMessage(position) => f.write_str(format!("{}", position).as_str()),
            UserMessage(user) => f.write_str(format!("{}", user).as_str()),
        }
    }
}

impl Default for MCMessage {
    fn default() -> Self {
        NewTextMessage(String::default())
    }
}

/// An entry in the Channel View that represents some type of message sent to either this user on
/// this device or to a channel this device can read. Can be any of [MCMessage] types.
#[allow(dead_code)] // Remove when the 'seen' field is used
#[derive(Clone, Debug, Default)]
pub struct ChannelViewEntry {
    /// NodeId of the node that sent this message
    from: u32,
    message_id: u32,
    /// The daytime the message was sent/received
    rx_daytime: DateTime<Local>,
    /// The message contents of differing types
    message: MCMessage,
    /// Has the user of the app seen this message?
    seen: bool,
    /// Has the entry been acknowledged as received by a receiver?
    acked: bool,
    /// Map of emojis and for each emoji there is the string for it and a number of node ids
    /// who sent that emoji
    emoji_reply: HashMap<String, Vec<u32>>,
}

impl ChannelViewEntry {
    /// Create a new [ChannelViewEntry] from the parameters provided. The received time will be set to
    /// the current time in EPOC as an u64
    pub fn new(message_id: u32, from: u32, message: MCMessage, rx_time: u32) -> Self {
        ChannelViewEntry {
            message,
            from,
            message_id,
            rx_daytime: Self::rx_time(rx_time),
            ..Default::default()
        }
    }

    /// Get the time now as a [DateTime<Local>]
    pub fn rx_time(rx_time: u32) -> DateTime<Local> {
        let datetime_utc = DateTime::<Utc>::from_timestamp_secs(rx_time as i64).unwrap();
        datetime_utc.with_timezone(&Local)
    }

    /// Return the node id that sent the message
    pub fn from(&self) -> u32 {
        self.from
    }

    /// Get a reference to the payload of this message
    pub fn message(&self) -> &MCMessage {
        &self.message
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

    /// Return true if the user has seen this message already
    pub fn seen(&self) -> bool {
        self.seen
    }

    /// Mark this message as having been seen by the user already
    pub fn mark_seen(&mut self) {
        self.seen = true;
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
            .map(|entry| format!("Re: {}", entry.message))
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
        nodes: &'a HashMap<u32, MCNodeInfo>,
        sources: &'a Vec<u32>,
    ) -> Element<'a, Message> {
        let mut col = Column::new();
        for source in sources {
            col = col.push(text(short_name(nodes, *source)).color(Self::color_from_id(*source)));
        }
        col.into()
    }

    fn tokenize<'a>(message: String) -> Vec<Span<'a, String>> {
        let mut spans = vec![];
        let mut pending_text = String::default();

        for word in message.split_inclusive(' ') {
            if !word.is_empty() {
                // if we find a link
                if word.starts_with("http:") || word.starts_with("https:") {
                    // If we have accumulated text so far, push it first in a single span
                    if !pending_text.is_empty() {
                        spans.push(span(pending_text.clone()));
                        pending_text.clear();
                    }
                    // Push a span for the link
                    spans.push(span(word.to_string()).link(word.to_string()));
                } else {
                    // Accumulate the word, with the space before the next word
                    pending_text.push_str(word);
                }
            }
        }

        // If we have accumulated any final text, push it all in a single span
        if !pending_text.is_empty() {
            spans.push(span(pending_text));
        }

        spans
    }

    /// Create an Element that contains a message received or sent
    pub fn view<'a>(
        &'a self,
        entries: &'a RingMap<u32, ChannelViewEntry>,
        nodes: &'a HashMap<u32, MCNodeInfo>,
        channel_id: &'a ChannelId,
        mine: bool,
        emoji_picker: &'a crate::emoji_picker::EmojiPicker,
    ) -> Element<'a, Message> {
        let message_text = self.message().to_string();
        let mut message_content_column = Column::new();

        if !mine {
            message_content_column = self.top_row(
                message_content_column,
                short_name(nodes, self.from),
                long_name(nodes, self.from),
                message_text.clone(),
                emoji_picker,
                channel_id,
            );
        }

        let content: Element<'static, Message> = match self.message() {
            AlertMessage(_) => rich_text(Self::tokenize(message_text))
                .style(alert_message_style)
                .size(18)
                .into(),
            NewTextMessage(_) | UserMessage(_) => rich_text(Self::tokenize(message_text))
                .on_link_click(OpenUrl)
                .style(message_text_style)
                .size(18)
                .into(),
            TextMessageReply(reply_to_id, _) => {
                if let Some(reply_quote) = Self::reply_quote(entries, reply_to_id) {
                    let quote_row = Row::new().push(text(reply_quote).color(COLOR_GREEN));
                    message_content_column = message_content_column.push(quote_row);
                };
                rich_text(Self::tokenize(message_text))
                    .style(message_text_style)
                    .size(18)
                    .into()
            }
            PositionMessage(position) => button(text(message_text))
                .padding([1, 5])
                .style(button_chip_style)
                .on_press(ShowLocation(position.clone()))
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
                DeviceViewEvent(ChannelMsg(channel_id.clone(), MessageSeen(self.message_id)))
            })
            .on_hide(DeviceViewEvent(ChannelMsg(
                channel_id.clone(),
                MessageUnseen(self.message_id),
            )))
            .into()
    }

    /// Add a row to the content column with the name of the source node
    fn top_row<'a>(
        &'a self,
        message_content_column: Column<'a, Message>,
        short_name: &'a str,
        long_name: &'a str,
        message: String,
        emoji_picker: &'a crate::emoji_picker::EmojiPicker,
        channel_id: &'a ChannelId,
    ) -> Column<'a, Message> {
        let text_color = Self::color_from_id(self.from);
        let mut top_row = Row::new().padding(0).align_y(Top);

        let short_name_text: Text = text(short_name)
            .font(Font {
                weight: Weight::Bold,
                ..Default::default()
            })
            .color(text_color);

        let short_name_tooltip: Tooltip<Message> =
            tooltip(short_name_text, text(long_name), tooltip::Position::Right)
                .style(tooltip_style);

        top_row = top_row
            .push(self.menu_bar(short_name, message, emoji_picker, channel_id))
            .push(Space::new().width(2.0))
            .push(short_name_tooltip);

        message_content_column.push(top_row)
    }

    /// Append an element to the column that contains the emoji replies for this message, if any.
    fn emoji_row<'a>(
        &'a self,
        nodes: &'a HashMap<u32, MCNodeInfo>,
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

    fn menu_bar<'a>(
        &'a self,
        name: &'a str,
        message: String,
        emoji_picker: &'a crate::emoji_picker::EmojiPicker,
        channel_id: &'a ChannelId,
    ) -> MenuBar<'a, Message, Theme, Renderer> {
        let menu_tpl_2 = |items| Menu::new(items).max_width(180.0).offset(15.0).spacing(5.0);

        let message_id = self.message_id;
        let dm = format!("DM with {}", name);

        let picker_element = emoji_picker
            .view(move |emoji| ReplyWithEmoji(message_id, emoji, channel_id.clone()))
            .map(move |picker_msg| {
                DeviceViewEvent(ChannelMsg(
                    channel_id.clone(),
                    ChannelViewMessage::EmojiPickerMsg(Box::new(picker_msg)),
                ))
            });

        #[rustfmt::skip]
        let menu_items = if matches!(channel_id, ChannelId::Channel(_)) {
            menu_items!(
                (button(Row::new().push(text("react")).push(Space::new().width(Fill)).push(text("▶"))).style(button_chip_style).width(Fill),
                menu_tpl_2(menu_items!(
                (picker_element)))),
                (menu_button("copy".into(), CopyToClipBoard(message.to_string()))),
                (menu_button("forward".into(), DeviceViewEvent(StartForwardingMessage(self.clone())))),
                (menu_button("reply".into(), DeviceViewEvent(ChannelMsg(channel_id.clone(), ChannelViewMessage::PrepareReply(self.message_id))))),
                (menu_button(dm, DeviceViewEvent(ShowChannel(Some(ChannelId::Node(self.from()))))))
            )
        } else {
            menu_items!(
                (button(Row::new().push(text("react")).push(Space::new().width(Fill)).push(text("▶"))).style(button_chip_style).width(Fill),
                menu_tpl_2(menu_items!(
                (picker_element)))),
                (menu_button("copy".into(), CopyToClipBoard(message.to_string()))),
                (menu_button("forward".into(), DeviceViewEvent(StartForwardingMessage(self.clone())))),
                (menu_button("reply".into(), DeviceViewEvent(ChannelMsg(channel_id.clone(), ChannelViewMessage::PrepareReply(self.message_id))))))
    };

        // Create the menu bar with the root button and list of options
        let menu_tpl_1 = |items| Menu::new(items).spacing(3);
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

#[cfg(test)]
mod tests {
    use crate::channel_view_entry::ChannelViewEntry;

    #[test]
    fn test_empty_spans() {
        assert!(ChannelViewEntry::tokenize("".into()).is_empty())
    }

    #[test]
    fn test_text_link_spans() {
        let spans = ChannelViewEntry::tokenize("open this link https://example.com".into());
        assert_eq!(spans.len(), 2);
        assert!(spans[0].link.is_none());
        assert!(spans[1].link.is_some());
    }

    #[test]
    fn test_text_link_text_spans() {
        let spans =
            ChannelViewEntry::tokenize("open this link https://example.com if you dare".into());
        assert_eq!(spans.len(), 3);
        assert!(spans[0].link.is_none());
        assert!(spans[1].link.is_some());
        assert!(spans[2].link.is_none());
    }
}
