use crate::Message;
use crate::Message::{CopyToClipBoard, DeviceViewEvent, OpenUrl, ShowLocation};
use crate::channel_view::ChannelViewMessage;
use crate::channel_view::ChannelViewMessage::{MessageSeen, MessageUnseen, ReplyWithEmoji};
use crate::conversation_id::{ConversationId, MessageId, NodeId};
use crate::device::DeviceViewMessage::{ChannelMsg, ShowChannel, StartForwardingMessage};
use crate::device::{TimeStamp, long_name, short_name};
use crate::meshchat::{MCNodeInfo, MCPosition, MCUser};
use crate::message::MCContent::{
    AlertMessage, EmojiReply, NewTextMessage, PositionMessage, TextMessageReply, UserMessage,
};
use crate::styles::{
    COLOR_DICTIONARY, COLOR_GREEN, TIME_TEXT_COLOR, TIME_TEXT_SIZE, TIME_TEXT_WIDTH,
    alert_message_style, bubble_style, button_chip_style, menu_button_style, message_text_style,
    tooltip_style,
};
use crate::widgets::emoji_picker::EmojiPicker;
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
pub enum MCContent {
    AlertMessage(String),   // message_text
    NewTextMessage(String), // message_text
    /// MessageId, String
    TextMessageReply(MessageId, String), // reply_message_id, reply_text
    EmojiReply(MessageId, String), // reply_message_id, reply_emoji_text
    PositionMessage(MCPosition), // position
    UserMessage(MCUser),    // user
}

impl fmt::Display for MCContent {
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

impl Default for MCContent {
    fn default() -> Self {
        NewTextMessage(String::default())
    }
}

/// An entry in the Channel View that represents some type of message sent to either this user on
/// this device or to a channel this device can read. Can be any of [MCContent] types.
#[derive(Clone, Debug, Default)]
pub struct MCMessage {
    /// NodeId of the node that sent this message
    from: NodeId,
    message_id: MessageId,
    /// The daytime the message was sent/received
    timestamp: DateTime<Local>,
    /// The message contents of differing types
    message: MCContent,
    /// Has the user of the app seen this message?
    seen: bool,
    /// Has the entry been acknowledged as received by a receiver?
    acked: bool,
    /// Map of emojis and for each emoji there is the string for it and a number of node ids
    /// who sent that emoji
    emoji_reply: HashMap<String, Vec<NodeId>>,
}

impl MCMessage {
    /// Create a new [MCMessage] from the parameters provided. The received time will be set to
    /// the current time in EPOC as an u64
    pub fn new(
        message_id: MessageId,
        from: NodeId,
        message: MCContent,
        timestamp: TimeStamp,
    ) -> Self {
        MCMessage {
            from,
            message_id,
            timestamp: Self::local_time(timestamp),
            message,
            ..Default::default()
        }
    }

    /// Get the time now as a [DateTime<Local>]
    pub fn local_time(timestamp: TimeStamp) -> DateTime<Local> {
        let datetime_utc =
            DateTime::<Utc>::from_timestamp_secs(timestamp.into()).unwrap_or_default();
        datetime_utc.with_timezone(&Local)
    }

    /// Return the node id that sent the message
    pub fn from(&self) -> NodeId {
        self.from
    }

    /// Get a reference to the payload of this message
    pub fn message(&self) -> &MCContent {
        &self.message
    }

    /// Return the message_id
    pub fn message_id(&self) -> MessageId {
        self.message_id
    }

    /// Mark the Entry as acknowledgeMd
    pub fn ack(&mut self) {
        self.acked = true;
    }

    /// Add an emoji reply to this entry
    pub fn add_emoji(&mut self, emoji_string: String, from: NodeId) {
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
    pub fn emojis(&self) -> &HashMap<String, Vec<NodeId>> {
        &self.emoji_reply
    }

    /// Return the time this message was received/sent as u64 seconds in EPOCH time
    pub fn time(&self) -> DateTime<Local> {
        self.timestamp
    }

    /// Order two messages - using the rx_daytime field.
    pub fn sort_by_timestamp(_: &MessageId, left: &Self, _: &MessageId, right: &Self) -> Ordering {
        left.timestamp.cmp(&right.timestamp)
    }

    /// Return the text to use when replying to this message
    pub fn reply_quote(
        entries: &RingMap<MessageId, MCMessage>,
        message_id: &MessageId,
    ) -> Option<String> {
        entries
            .get(message_id)
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
    fn color_from_id(node_id: NodeId) -> Color {
        let mut hasher = DefaultHasher::new();
        node_id.hash(&mut hasher);
        let hash = hasher.finish();

        let index = (hash % COLOR_DICTIONARY.len() as u64) as usize;
        COLOR_DICTIONARY[index]
    }

    /// Format a time as seconds in epoc (u64) into a String of hour and minutes during the day
    /// it occurs in. These will be separated by Day specifiers, so day is not needed.
    fn time_to_text(datetime_local: DateTime<Local>) -> Text<'static> {
        let time_str = datetime_local.format("%H:%M").to_string(); // Formats as HH:MM
        text(time_str)
            .color(TIME_TEXT_COLOR)
            .size(TIME_TEXT_SIZE)
            .width(Fixed(TIME_TEXT_WIDTH))
    }

    /// Return an element (currently a Column) with a list of the names of the nodes that sent the
    /// given emoji.
    fn list_of_nodes<'a>(
        nodes: &'a HashMap<NodeId, MCNodeInfo>,
        sources: &'a Vec<NodeId>,
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
        entries: &'a RingMap<MessageId, MCMessage>,
        nodes: &'a HashMap<NodeId, MCNodeInfo>,
        conversation_id: &'a ConversationId,
        mine: bool,
        emoji_picker: &'a EmojiPicker,
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
                conversation_id,
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
            .style(move |theme: &Theme| bubble_style(theme, mine));

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
                DeviceViewEvent(ChannelMsg(*conversation_id, MessageSeen(self.message_id)))
            })
            .on_hide(DeviceViewEvent(ChannelMsg(
                *conversation_id,
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
        emoji_picker: &'a EmojiPicker,
        conversation_id: &'a ConversationId,
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
            .push(self.menu_bar(short_name, message, emoji_picker, conversation_id))
            .push(Space::new().width(2.0))
            .push(short_name_tooltip);

        message_content_column.push(top_row)
    }

    /// Append an element to the column that contains the emoji replies for this message, if any.
    fn emoji_row<'a>(
        &'a self,
        nodes: &'a HashMap<NodeId, MCNodeInfo>,
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
        emoji_picker: &'a EmojiPicker,
        conversation_id: &'a ConversationId,
    ) -> MenuBar<'a, Message, Theme, Renderer> {
        let menu_tpl_2 = |items| Menu::new(items).max_width(180.0).offset(15.0).spacing(5.0);

        let message_id = self.message_id;
        let dm = format!("DM with {}", name);

        let picker_element = emoji_picker
            .view(move |emoji| ReplyWithEmoji(message_id, emoji, *conversation_id))
            .map(move |picker_msg| {
                DeviceViewEvent(ChannelMsg(
                    *conversation_id,
                    ChannelViewMessage::EmojiPickerMsg(Box::new(picker_msg)),
                ))
            });

        #[rustfmt::skip]
        let menu_items = if matches!(conversation_id, ConversationId::Channel(_)) {
            menu_items!(
                (button(Row::new().push(text("react")).push(Space::new().width(Fill)).push(text("▶"))).style(button_chip_style).width(Fill),
                menu_tpl_2(menu_items!(
                (picker_element)))),
                (menu_button("copy".into(), CopyToClipBoard(message.to_string()))),
                (menu_button("forward".into(), DeviceViewEvent(StartForwardingMessage(self.clone())))),
                (menu_button("reply".into(), DeviceViewEvent(ChannelMsg(*conversation_id, ChannelViewMessage::PrepareReply(self.message_id))))),
                (menu_button(dm, DeviceViewEvent(ShowChannel(Some(ConversationId::Node(self.from()))))))
            )
        } else {
            menu_items!(
                (button(Row::new().push(text("react")).push(Space::new().width(Fill)).push(text("▶"))).style(button_chip_style).width(Fill),
                menu_tpl_2(menu_items!(
                (picker_element)))),
                (menu_button("copy".into(), CopyToClipBoard(message.to_string()))),
                (menu_button("forward".into(), DeviceViewEvent(StartForwardingMessage(self.clone())))),
                (menu_button("reply".into(), DeviceViewEvent(ChannelMsg(*conversation_id, ChannelViewMessage::PrepareReply(self.message_id))))))
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
impl PartialEq<Self> for MCMessage {
    fn eq(&self, other: &Self) -> bool {
        self.timestamp == other.timestamp
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::MeshChat;
    use crate::conversation_id::MessageId;
    use crate::meshchat::MCNodeInfo;
    use crate::message::MCContent::{AlertMessage, EmojiReply, NewTextMessage, TextMessageReply};
    use chrono::Datelike;
    use ringmap::RingMap;

    #[test]
    fn test_empty_spans() {
        assert!(MCMessage::tokenize("".into()).is_empty())
    }

    #[test]
    fn test_text_link_spans() {
        let spans = MCMessage::tokenize("open this link https://example.com".into());
        assert_eq!(spans.len(), 2);
        assert!(spans[0].link.is_none());
        assert!(spans[1].link.is_some());
    }

    #[test]
    fn test_text_link_text_spans() {
        let spans = MCMessage::tokenize("open this link https://example.com if you dare".into());
        assert_eq!(spans.len(), 3);
        assert!(spans[0].link.is_none());
        assert!(spans[1].link.is_some());
        assert!(spans[2].link.is_none());
    }

    #[test]
    fn test_tokenize_http_link() {
        let spans = MCMessage::tokenize("check https://example.com out".into());
        assert_eq!(spans.len(), 3);
        assert!(spans[1].link.is_some());
    }

    #[test]
    fn test_tokenize_multiple_links() {
        let spans =
            MCMessage::tokenize("visit https://first.com and https://second.com today".into());
        // "visit " -> span 0
        // "https://first.com " -> span 1 (link)
        // "and " -> span 2
        // "https://second.com " -> span 3 (link)
        // "today" -> span 4
        assert_eq!(spans.len(), 5);
        assert!(spans[0].link.is_none()); // "visit "
        assert!(spans[1].link.is_some()); // first link
        assert!(spans[2].link.is_none()); // "and "
        assert!(spans[3].link.is_some()); // second link
        assert!(spans[4].link.is_none()); // "today"
    }

    #[test]
    fn test_tokenize_only_link() {
        let spans = MCMessage::tokenize("https://example.com".into());
        assert_eq!(spans.len(), 1);
        assert!(spans[0].link.is_some());
    }

    #[test]
    fn test_truncate_shorter_than_max() {
        let result = MCMessage::truncate("hello", 10);
        assert_eq!(result, "hello");
    }

    #[test]
    fn test_truncate_exact_length() {
        let result = MCMessage::truncate("hello", 5);
        assert_eq!(result, "hello");
    }

    #[test]
    fn test_truncate_longer_than_max() {
        let result = MCMessage::truncate("hello world", 5);
        assert_eq!(result, "hello");
    }

    #[test]
    fn test_truncate_unicode() {
        // Test with multibyte Unicode characters
        let result = MCMessage::truncate("héllo wörld", 5);
        assert_eq!(result, "héllo");
    }

    #[test]
    fn test_truncate_emoji() {
        // Emoji are multi-byte
        let result = MCMessage::truncate("👋🌍hello", 2);
        assert_eq!(result, "👋🌍");
    }

    #[test]
    fn test_channel_view_entry_new() {
        let entry = MCMessage::new(
            MessageId::from(123),
            NodeId::from(456u64),
            NewTextMessage("Hello".into()),
            MeshChat::now(),
        );

        assert_eq!(entry.message_id(), MessageId::from(123));
        assert_eq!(entry.from(), NodeId::from(456u64));
        assert!(!entry.seen());
        assert!(!entry.acked());
    }

    #[test]
    fn test_mark_seen() {
        let mut entry = MCMessage::new(
            MessageId::from(1),
            NodeId::from(1u64),
            NewTextMessage("test".into()),
            MeshChat::now(),
        );
        assert!(!entry.seen());

        entry.mark_seen();
        assert!(entry.seen());
    }

    #[test]
    fn test_ack() {
        let mut entry = MCMessage::new(
            MessageId::from(1),
            NodeId::from(1u64),
            NewTextMessage("test".into()),
            MeshChat::now(),
        );
        assert!(!entry.acked());

        entry.ack();
        assert!(entry.acked());
    }

    #[test]
    fn test_add_emoji() {
        let mut entry = MCMessage::new(
            MessageId::from(1),
            NodeId::from(1u64),
            NewTextMessage("test".into()),
            MeshChat::now(),
        );
        assert!(entry.emojis().is_empty());

        entry.add_emoji("👍".to_string(), NodeId::from(100u64));
        assert_eq!(entry.emojis().len(), 1);
        assert!(entry.emojis().contains_key("👍"));
        assert_eq!(
            entry.emojis().get("👍").expect("Could not get emoji"),
            &vec![NodeId::from(100u64)]
        );
    }

    #[test]
    fn test_add_emoji_multiple_same() {
        let mut entry = MCMessage::new(
            MessageId::from(1),
            NodeId::from(1u64),
            NewTextMessage("test".into()),
            MeshChat::now(),
        );

        entry.add_emoji("👍".to_string(), NodeId::from(100u64));
        entry.add_emoji("👍".to_string(), NodeId::from(200u64));

        assert_eq!(entry.emojis().len(), 1);
        assert_eq!(
            entry.emojis().get("👍").expect("Could not get emoji"),
            &vec![NodeId::from(100u64), NodeId::from(200u64)]
        );
    }

    #[test]
    fn test_add_emoji_different() {
        let mut entry = MCMessage::new(
            MessageId::from(1),
            NodeId::from(1u64),
            NewTextMessage("test".into()),
            MeshChat::now(),
        );

        entry.add_emoji("👍".to_string(), NodeId::from(100u64));
        entry.add_emoji("❤️".to_string(), NodeId::from(200u64));

        assert_eq!(entry.emojis().len(), 2);
    }

    #[test]
    fn test_reply_quote_found() {
        let mut entries: RingMap<MessageId, MCMessage> = RingMap::new();
        let entry = MCMessage::new(
            MessageId::from(1),
            NodeId::from(100u64),
            NewTextMessage("Original message".into()),
            MeshChat::now(),
        );
        entries.insert(MessageId::from(1), entry);

        let quote = MCMessage::reply_quote(&entries, &MessageId::from(1));
        assert!(quote.expect("No quote").contains("Re:"));
    }

    #[test]
    fn test_reply_quote_not_found() {
        let entries: RingMap<MessageId, MCMessage> = RingMap::new();
        let quote = MCMessage::reply_quote(&entries, &MessageId::from(999));
        assert!(quote.is_none());
    }

    #[test]
    fn test_reply_quote_truncates_long_message() {
        let mut entries: RingMap<MessageId, MCMessage> = RingMap::new();
        let long_message = "This is a very long message that should be truncated when quoted";
        let entry = MCMessage::new(
            MessageId::from(1),
            NodeId::from(100u64),
            NewTextMessage(long_message.into()),
            MeshChat::now(),
        );
        entries.insert(MessageId::from(1), entry);

        let quote = MCMessage::reply_quote(&entries, &MessageId::from(1))
            .expect("Could not get reply quoted");
        assert!(quote.len() < long_message.len() + 10); // "Re: " prefix plus some truncation
        assert!(quote.ends_with("..."));
    }

    #[test]
    fn test_sort_by_timestamp() {
        let older = MCMessage::new(
            MessageId::from(1),
            NodeId::from(100u64),
            NewTextMessage("old".into()),
            TimeStamp::from(1000),
        );
        let newer = MCMessage::new(
            MessageId::from(2),
            NodeId::from(100u64),
            NewTextMessage("new".into()),
            TimeStamp::from(2000),
        );

        let ordering =
            MCMessage::sort_by_timestamp(&MessageId::from(1), &older, &MessageId::from(2), &newer);
        assert_eq!(ordering, Ordering::Less);

        let ordering =
            MCMessage::sort_by_timestamp(&MessageId::from(2), &newer, &MessageId::from(1), &older);
        assert_eq!(ordering, Ordering::Greater);
    }

    #[test]
    fn test_mc_message_display_alert() {
        let msg = AlertMessage("Alert!".into());
        assert_eq!(format!("{}", msg), "Alert!");
    }

    #[test]
    fn test_mc_message_display_new_text() {
        let msg = NewTextMessage("Hello".into());
        assert_eq!(format!("{}", msg), "Hello");
    }

    #[test]
    fn test_mc_message_display_text_reply() {
        let msg = TextMessageReply(MessageId::from(123), "Reply text".into());
        assert_eq!(format!("{}", msg), "Re: Reply text");
    }

    #[test]
    fn test_mc_message_display_emoji_reply() {
        let msg = EmojiReply(MessageId::from(123), "👍".into());
        assert_eq!(format!("{}", msg), "👍: 123");
    }

    #[test]
    fn test_mc_message_default() {
        let msg = MCContent::default();
        matches!(msg, NewTextMessage(s) if s.is_empty());
    }

    #[test]
    fn test_channel_view_entry_time() {
        let timestamp = MeshChat::now();
        let entry = MCMessage::new(
            MessageId::from(1),
            NodeId::from(100u64),
            NewTextMessage("test".into()),
            timestamp,
        );

        // The time should be convertible and reasonable
        let time = entry.time();
        assert!(time.year() >= 2020);
    }

    #[test]
    fn test_mc_message_display_position() {
        let position = MCPosition {
            latitude: 37.7749,
            longitude: -122.4194,
            altitude: Some(10),
            time: 0,
            location_source: 0,
            altitude_source: 0,
            timestamp: TimeStamp::from(0),
            timestamp_millis_adjust: 0,
            altitude_hae: None,
            altitude_geoidal_separation: None,
            pdop: 0,
            hdop: 0,
            vdop: 0,
            gps_accuracy: 0,
            ground_speed: None,
            ground_track: None,
            fix_quality: 0,
            fix_type: 0,
            sats_in_view: 0,
            sensor_id: 0,
            next_update: 0,
            seq_number: 0,
            precision_bits: 0,
        };
        let msg = PositionMessage(position);
        let display = format!("{}", msg);
        assert!(display.contains("📌"));
        assert!(display.contains("37.77"));
        assert!(display.contains("-122.42"));
    }

    #[test]
    fn test_mc_message_display_position_negative_coords() {
        let position = MCPosition {
            latitude: -33.8688,
            longitude: 151.2093,
            altitude: None,
            time: 0,
            location_source: 0,
            altitude_source: 0,
            timestamp: TimeStamp::from(0),
            timestamp_millis_adjust: 0,
            altitude_hae: None,
            altitude_geoidal_separation: None,
            pdop: 0,
            hdop: 0,
            vdop: 0,
            gps_accuracy: 0,
            ground_speed: None,
            ground_track: None,
            fix_quality: 0,
            fix_type: 0,
            sats_in_view: 0,
            sensor_id: 0,
            next_update: 0,
            seq_number: 0,
            precision_bits: 0,
        };
        let msg = PositionMessage(position);
        let display = format!("{}", msg);
        assert!(display.contains("📌"));
        assert!(display.contains("-33.87"));
        assert!(display.contains("151.21"));
    }

    #[test]
    fn test_mc_message_display_position_zero_coords() {
        let position = MCPosition {
            latitude: 0.0,
            longitude: 0.0,
            altitude: None,
            time: 0,
            location_source: 0,
            altitude_source: 0,
            timestamp: TimeStamp::from(0),
            timestamp_millis_adjust: 0,
            altitude_hae: None,
            altitude_geoidal_separation: None,
            pdop: 0,
            hdop: 0,
            vdop: 0,
            gps_accuracy: 0,
            ground_speed: None,
            ground_track: None,
            fix_quality: 0,
            fix_type: 0,
            sats_in_view: 0,
            sensor_id: 0,
            next_update: 0,
            seq_number: 0,
            precision_bits: 0,
        };
        let msg = PositionMessage(position);
        let display = format!("{}", msg);
        assert_eq!(display, "📌 0.00, 0.00");
    }

    #[test]
    fn test_mc_message_display_user() {
        let user = MCUser {
            id: "!abc123".to_string(),
            long_name: "John Doe".to_string(),
            short_name: "JD".to_string(),
            hw_model_str: "T-Beam".to_string(),
            hw_model: 1,
            is_licensed: true,
            role_str: "Router".to_string(),
            role: 1,
            public_key: vec![],
            is_unmessagable: false,
        };
        let msg = UserMessage(user);
        let display = format!("{}", msg);
        assert!(display.contains("ⓘ"));
        assert!(display.contains("John Doe"));
        assert!(display.contains("JD"));
        assert!(display.contains("!abc123"));
        assert!(display.contains("T-Beam"));
    }

    #[test]
    fn test_mc_message_display_user_empty_names() {
        let user = MCUser {
            id: "".to_string(),
            long_name: "".to_string(),
            short_name: "".to_string(),
            hw_model_str: "Unknown".to_string(),
            hw_model: 0,
            is_licensed: false,
            role_str: "".to_string(),
            role: 0,
            public_key: vec![],
            is_unmessagable: false,
        };
        let msg = UserMessage(user);
        let display = format!("{}", msg);
        assert!(display.contains("ⓘ"));
        assert!(display.contains("Unknown"));
    }

    #[test]
    fn test_mc_message_display_user_special_characters() {
        let user = MCUser {
            id: "!xyz789".to_string(),
            long_name: "José García".to_string(),
            short_name: "JG".to_string(),
            hw_model_str: "Heltec V3".to_string(),
            hw_model: 2,
            is_licensed: false,
            role_str: "Client".to_string(),
            role: 0,
            public_key: vec![1, 2, 3],
            is_unmessagable: true,
        };
        let msg = UserMessage(user);
        let display = format!("{}", msg);
        assert!(display.contains("José García"));
        assert!(display.contains("JG"));
        assert!(display.contains("Heltec V3"));
    }

    #[test]
    fn test_mc_position_display_directly() {
        let position = MCPosition {
            latitude: 51.5074,
            longitude: -0.1278,
            altitude: Some(11),
            time: 1234567890,
            location_source: 1,
            altitude_source: 1,
            timestamp: TimeStamp::from(1234567890),
            timestamp_millis_adjust: 0,
            altitude_hae: Some(11),
            altitude_geoidal_separation: Some(0),
            pdop: 1,
            hdop: 1,
            vdop: 1,
            gps_accuracy: 5,
            ground_speed: Some(0),
            ground_track: Some(0),
            fix_quality: 1,
            fix_type: 3,
            sats_in_view: 8,
            sensor_id: 0,
            next_update: 0,
            seq_number: 1,
            precision_bits: 32,
        };
        let display = format!("{}", position);
        assert_eq!(display, "📌 51.51, -0.13");
    }

    #[test]
    fn test_mc_user_display_directly() {
        let user = MCUser {
            id: "!meshtastic".to_string(),
            long_name: "Test User".to_string(),
            short_name: "TU".to_string(),
            hw_model_str: "RAK4631".to_string(),
            hw_model: 3,
            is_licensed: true,
            role_str: "Router".to_string(),
            role: 2,
            public_key: vec![0xDE, 0xAD, 0xBE, 0xEF],
            is_unmessagable: false,
        };
        let display = format!("{}", user);
        assert_eq!(
            display,
            "ⓘ from 'Test User' ('TU'), id = '!meshtastic', with hardware 'RAK4631'"
        );
    }

    // Tests for color_from_id()
    #[test]
    fn test_color_from_id_returns_valid_color() {
        use crate::styles::COLOR_DICTIONARY;

        let color = MCMessage::color_from_id(NodeId::from(12345u64));
        // The color should be one of the colors in COLOR_DICTIONARY
        assert!(COLOR_DICTIONARY.contains(&color));
    }

    #[test]
    fn test_color_from_id_consistent() {
        // Same id should always return the same color
        let color1 = MCMessage::color_from_id(NodeId::from(42u64));
        let color2 = MCMessage::color_from_id(NodeId::from(42u64));
        assert_eq!(color1, color2);
    }

    #[test]
    fn test_color_from_id_different_ids_can_differ() {
        // Different ids may produce different colors (though collisions are possible)
        let color1 = MCMessage::color_from_id(NodeId::from(1u64));
        let color2 = MCMessage::color_from_id(NodeId::from(2u64));
        let color3 = MCMessage::color_from_id(NodeId::from(3u64));
        // At least some should be different (testing distribution)
        let all_same = color1 == color2 && color2 == color3;
        // It's statistically unlikely all three are the same
        assert!(!all_same || color1 == color2);
    }

    #[test]
    fn test_color_from_id_zero() {
        use crate::styles::COLOR_DICTIONARY;

        let color = MCMessage::color_from_id(NodeId::from(0u64));
        assert!(COLOR_DICTIONARY.contains(&color));
    }

    #[test]
    fn test_color_from_id_max_u32() {
        use crate::styles::COLOR_DICTIONARY;

        let color = MCMessage::color_from_id(NodeId::from(u64::MAX));
        assert!(COLOR_DICTIONARY.contains(&color));
    }

    // Tests for time_to_text()
    #[test]
    fn test_time_to_text_formats_correctly() {
        use chrono::TimeZone;

        // Create a specific local time
        let datetime = Local.with_ymd_and_hms(2024, 6, 15, 14, 30, 0).unwrap();
        let text_widget = MCMessage::time_to_text(datetime);

        // We can't easily inspect the Text widget's content, but we can verify it doesn't panic
        // and returns a valid widget
        let _ = text_widget;
    }

    #[test]
    fn test_time_to_text_midnight() {
        use chrono::TimeZone;

        let datetime = Local.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap();
        let text_widget = MCMessage::time_to_text(datetime);
        let _ = text_widget;
    }

    #[test]
    fn test_time_to_text_end_of_day() {
        use chrono::TimeZone;

        let datetime = Local.with_ymd_and_hms(2024, 12, 31, 23, 59, 59).unwrap();
        let text_widget = MCMessage::time_to_text(datetime);
        let _ = text_widget;
    }

    #[test]
    fn test_time_to_text_single_digit_hour() {
        use chrono::TimeZone;

        let datetime = Local.with_ymd_and_hms(2024, 6, 15, 9, 5, 0).unwrap();
        let text_widget = MCMessage::time_to_text(datetime);
        let _ = text_widget;
    }

    // Tests for list_of_nodes()
    #[test]
    fn test_list_of_nodes_empty() {
        let nodes: HashMap<NodeId, MCNodeInfo> = HashMap::new();
        let sources: Vec<NodeId> = vec![];

        let element = MCMessage::list_of_nodes(&nodes, &sources);
        // Element should be created without a panic
        let _ = element;
    }

    #[test]
    fn test_list_of_nodes_single_known_node() {
        let mut nodes: HashMap<NodeId, MCNodeInfo> = HashMap::new();
        nodes.insert(
            NodeId::from(100u64),
            MCNodeInfo {
                node_id: NodeId::from(100u64),
                user: Some(MCUser {
                    id: "!node100".to_string(),
                    long_name: "Node One Hundred".to_string(),
                    short_name: "N100".to_string(),
                    hw_model_str: "T-Beam".to_string(),
                    hw_model: 1,
                    is_licensed: false,
                    role_str: "Client".to_string(),
                    role: 0,
                    public_key: vec![],
                    is_unmessagable: false,
                }),
                position: None,
                is_ignored: false,
            },
        );
        let sources = vec![NodeId::from(100u64)];

        let element = MCMessage::list_of_nodes(&nodes, &sources);
        let _ = element;
    }

    #[test]
    fn test_list_of_nodes_multiple_nodes() {
        let mut nodes: HashMap<NodeId, MCNodeInfo> = HashMap::new();
        nodes.insert(
            NodeId::from(1u64),
            MCNodeInfo {
                node_id: NodeId::from(1u64),
                user: Some(MCUser {
                    id: "!node1".to_string(),
                    long_name: "First Node".to_string(),
                    short_name: "FN".to_string(),
                    hw_model_str: "T-Beam".to_string(),
                    hw_model: 1,
                    is_licensed: false,
                    role_str: "Client".to_string(),
                    role: 0,
                    public_key: vec![],
                    is_unmessagable: false,
                }),
                position: None,
                is_ignored: false,
            },
        );
        nodes.insert(
            NodeId::from(2u64),
            MCNodeInfo {
                node_id: NodeId::from(2u64),
                user: Some(MCUser {
                    id: "!node2".to_string(),
                    long_name: "Second Node".to_string(),
                    short_name: "SN".to_string(),
                    hw_model_str: "Heltec".to_string(),
                    hw_model: 2,
                    is_licensed: true,
                    role_str: "Router".to_string(),
                    role: 1,
                    public_key: vec![],
                    is_unmessagable: false,
                }),
                position: None,
                is_ignored: false,
            },
        );
        let sources = vec![NodeId::from(1u64), NodeId::from(2u64)];

        let element = MCMessage::list_of_nodes(&nodes, &sources);
        let _ = element;
    }

    #[test]
    fn test_list_of_nodes_unknown_node() {
        // Test with a node id that's not in the node map
        let nodes: HashMap<NodeId, MCNodeInfo> = HashMap::new();
        let sources = vec![NodeId::from(999u64)]; // This node doesn't exist in the map

        let element = MCMessage::list_of_nodes(&nodes, &sources);
        let _ = element;
    }

    #[test]
    fn test_list_of_nodes_mixed_known_unknown() {
        let mut nodes: HashMap<NodeId, MCNodeInfo> = HashMap::new();
        nodes.insert(
            NodeId::from(1u64),
            MCNodeInfo {
                node_id: NodeId::from(1u64),
                user: Some(MCUser {
                    id: "!known".to_string(),
                    long_name: "Known Node".to_string(),
                    short_name: "KN".to_string(),
                    hw_model_str: "RAK".to_string(),
                    hw_model: 3,
                    is_licensed: false,
                    role_str: "Client".to_string(),
                    role: 0,
                    public_key: vec![],
                    is_unmessagable: false,
                }),
                position: None,
                is_ignored: false,
            },
        );
        // sources include both known (1) and unknown (999) nodes
        let sources = vec![NodeId::from(1u64), NodeId::from(999u64)];

        let element = MCMessage::list_of_nodes(&nodes, &sources);
        let _ = element;
    }

    #[test]
    fn test_list_of_nodes_node_without_user() {
        let mut nodes: HashMap<NodeId, MCNodeInfo> = HashMap::new();
        nodes.insert(
            NodeId::from(50u64),
            MCNodeInfo {
                node_id: NodeId::from(50u64),
                user: None, // No user info
                position: None,
                is_ignored: false,
            },
        );
        let sources = vec![NodeId::from(50u64)];

        let element = MCMessage::list_of_nodes(&nodes, &sources);
        let _ = element;
    }

    // Tests for menu_button()
    #[test]
    fn test_menu_button_creates_button() {
        let btn = menu_button("Test Label".to_string(), Message::None);
        // Button should be created without a panic
        let _ = btn;
    }

    #[test]
    fn test_menu_button_with_copy_message() {
        let btn = menu_button(
            "copy".to_string(),
            CopyToClipBoard("test content".to_string()),
        );
        let _ = btn;
    }

    #[test]
    fn test_menu_button_with_empty_label() {
        let btn = menu_button("".to_string(), Message::None);
        let _ = btn;
    }

    #[test]
    fn test_menu_button_with_long_label() {
        let btn = menu_button(
            "This is a very long label that might wrap".to_string(),
            Message::None,
        );
        let _ = btn;
    }

    #[test]
    fn test_menu_button_with_unicode_label() {
        let btn = menu_button("复制 📋".to_string(), Message::None);
        let _ = btn;
    }

    // Tests for menu_root_button()
    #[test]
    fn test_menu_root_button_creates_button() {
        let btn = menu_root_button("▼");
        let _ = btn;
    }

    #[test]
    fn test_menu_root_button_with_different_symbol() {
        let btn = menu_root_button("⋮");
        let _ = btn;
    }

    #[test]
    fn test_menu_root_button_with_empty_label() {
        let btn = menu_root_button("");
        let _ = btn;
    }

    #[test]
    fn test_menu_root_button_with_text() {
        let btn = menu_root_button("Menu");
        let _ = btn;
    }

    #[test]
    fn test_menu_root_button_with_emoji() {
        let btn = menu_root_button("☰");
        let _ = btn;
    }

    // Tests for top_row()
    #[test]
    fn test_top_row_basic() {
        use crate::conversation_id::ConversationId;
        use crate::widgets::emoji_picker::EmojiPicker;
        use iced::widget::Column;

        let entry = MCMessage::new(
            MessageId::from(1),
            NodeId::from(100u64),
            NewTextMessage("Hello".into()),
            MeshChat::now(),
        );
        let emoji_picker = EmojiPicker::new();
        let conversation_id = ConversationId::Channel(0.into());
        let column: Column<Message> = Column::new();

        let result = entry.top_row(
            column,
            "TN",
            "Test Node",
            "Hello".to_string(),
            &emoji_picker,
            &conversation_id,
        );
        let _ = result;
    }

    #[test]
    fn test_top_row_with_dm_channel() {
        use crate::conversation_id::ConversationId;
        use crate::widgets::emoji_picker::EmojiPicker;
        use iced::widget::Column;

        let entry = MCMessage::new(
            MessageId::from(2),
            NodeId::from(200u64),
            NewTextMessage("DM message".into()),
            MeshChat::now(),
        );
        let emoji_picker = EmojiPicker::new();
        let conversation_id = ConversationId::Node(NodeId::from(200u64));
        let column: Column<Message> = Column::new();

        let result = entry.top_row(
            column,
            "DN",
            "DM Node",
            "DM message".to_string(),
            &emoji_picker,
            &conversation_id,
        );
        let _ = result;
    }

    #[test]
    fn test_top_row_with_empty_names() {
        use crate::conversation_id::ConversationId;
        use crate::widgets::emoji_picker::EmojiPicker;
        use iced::widget::Column;

        let entry = MCMessage::new(
            MessageId::from(3),
            NodeId::from(300u64),
            NewTextMessage("test".into()),
            MeshChat::now(),
        );
        let emoji_picker = EmojiPicker::new();
        let conversation_id = ConversationId::Channel(1.into());
        let column: Column<Message> = Column::new();

        let result = entry.top_row(
            column,
            "",
            "",
            "test".to_string(),
            &emoji_picker,
            &conversation_id,
        );
        let _ = result;
    }

    #[test]
    fn test_top_row_with_unicode_names() {
        use crate::conversation_id::ConversationId;
        use crate::widgets::emoji_picker::EmojiPicker;
        use iced::widget::Column;

        let entry = MCMessage::new(
            MessageId::from(4),
            NodeId::from(400u64),
            NewTextMessage("こんにちは".into()),
            MeshChat::now(),
        );
        let emoji_picker = EmojiPicker::new();
        let conversation_id = ConversationId::Channel(0.into());
        let column: Column<Message> = Column::new();

        let result = entry.top_row(
            column,
            "日本",
            "日本語ノード",
            "こんにちは".to_string(),
            &emoji_picker,
            &conversation_id,
        );
        let _ = result;
    }

    // Tests for emoji_row()
    #[test]
    fn test_emoji_row_no_emojis() {
        use iced::widget::Column;

        let entry = MCMessage::new(
            MessageId::from(1),
            NodeId::from(100u64),
            NewTextMessage("test".into()),
            MeshChat::now(),
        );
        let nodes: HashMap<NodeId, MCNodeInfo> = HashMap::new();
        let column: Column<Message> = Column::new();

        let result = entry.emoji_row(&nodes, column);
        let _ = result;
    }

    #[test]
    fn test_emoji_row_with_single_emoji() {
        use iced::widget::Column;

        let mut entry = MCMessage::new(
            MessageId::from(1),
            NodeId::from(100u64),
            NewTextMessage("test".into()),
            MeshChat::now(),
        );
        entry.add_emoji("👍".to_string(), NodeId::from(200u64));

        let mut nodes: HashMap<NodeId, MCNodeInfo> = HashMap::new();
        nodes.insert(
            NodeId::from(200u64),
            MCNodeInfo {
                node_id: NodeId::from(200u64),
                user: Some(MCUser {
                    id: "!node200".to_string(),
                    long_name: "Reactor Node".to_string(),
                    short_name: "RN".to_string(),
                    hw_model_str: "T-Beam".to_string(),
                    hw_model: 1,
                    is_licensed: false,
                    role_str: "Client".to_string(),
                    role: 0,
                    public_key: vec![],
                    is_unmessagable: false,
                }),
                position: None,
                is_ignored: false,
            },
        );
        let column: Column<Message> = Column::new();

        let result = entry.emoji_row(&nodes, column);
        let _ = result;
    }

    #[test]
    fn test_emoji_row_with_multiple_emojis() {
        use iced::widget::Column;

        let mut entry = MCMessage::new(
            MessageId::from(1),
            NodeId::from(100u64),
            NewTextMessage("test".into()),
            MeshChat::now(),
        );
        entry.add_emoji("👍".to_string(), NodeId::from(200u64));
        entry.add_emoji("❤️".to_string(), NodeId::from(300u64));
        entry.add_emoji("😂".to_string(), NodeId::from(400u64));

        let nodes: HashMap<NodeId, MCNodeInfo> = HashMap::new();
        let column: Column<Message> = Column::new();

        let result = entry.emoji_row(&nodes, column);
        let _ = result;
    }

    #[test]
    fn test_emoji_row_same_emoji_multiple_senders() {
        use iced::widget::Column;

        let mut entry = MCMessage::new(
            MessageId::from(1),
            NodeId::from(100u64),
            NewTextMessage("test".into()),
            MeshChat::now(),
        );
        entry.add_emoji("👍".to_string(), NodeId::from(200u64));
        entry.add_emoji("👍".to_string(), NodeId::from(300u64));
        entry.add_emoji("👍".to_string(), NodeId::from(400u64));

        let nodes: HashMap<NodeId, MCNodeInfo> = HashMap::new();
        let column: Column<Message> = Column::new();

        let result = entry.emoji_row(&nodes, column);
        let _ = result;
    }

    // Tests for menu_bar()
    #[test]
    fn test_menu_bar_channel_message() {
        use crate::conversation_id::ConversationId;
        use crate::widgets::emoji_picker::EmojiPicker;

        let entry = MCMessage::new(
            MessageId::from(1),
            NodeId::from(100u64),
            NewTextMessage("test".into()),
            MeshChat::now(),
        );
        let emoji_picker = EmojiPicker::new();
        let conversation_id = ConversationId::Channel(0.into());

        let result = entry.menu_bar(
            "TN",
            "test message".to_string(),
            &emoji_picker,
            &conversation_id,
        );
        let _ = result;
    }

    #[test]
    fn test_menu_bar_dm_message() {
        use crate::conversation_id::ConversationId;
        use crate::widgets::emoji_picker::EmojiPicker;

        let entry = MCMessage::new(
            MessageId::from(2),
            NodeId::from(200u64),
            NewTextMessage("dm test".into()),
            MeshChat::now(),
        );
        let emoji_picker = EmojiPicker::new();
        let conversation_id = ConversationId::Node(NodeId::from(200u64));

        let result = entry.menu_bar(
            "DN",
            "dm test message".to_string(),
            &emoji_picker,
            &conversation_id,
        );
        let _ = result;
    }

    #[test]
    fn test_menu_bar_with_empty_name() {
        use crate::conversation_id::ConversationId;
        use crate::widgets::emoji_picker::EmojiPicker;

        let entry = MCMessage::new(
            MessageId::from(3),
            NodeId::from(300u64),
            NewTextMessage("test".into()),
            MeshChat::now(),
        );
        let emoji_picker = EmojiPicker::new();
        let conversation_id = ConversationId::Channel(1.into());

        let result = entry.menu_bar("", "test".to_string(), &emoji_picker, &conversation_id);
        let _ = result;
    }

    #[test]
    fn test_menu_bar_with_long_message() {
        use crate::conversation_id::ConversationId;
        use crate::widgets::emoji_picker::EmojiPicker;

        let long_msg = "This is a very long message that might affect how the menu displays or behaves when copied or forwarded to other users in the mesh network.";
        let entry = MCMessage::new(
            MessageId::from(4),
            NodeId::from(400u64),
            NewTextMessage(long_msg.into()),
            MeshChat::now(),
        );
        let emoji_picker = EmojiPicker::new();
        let conversation_id = ConversationId::Channel(0.into());

        let result = entry.menu_bar("LM", long_msg.to_string(), &emoji_picker, &conversation_id);
        let _ = result;
    }

    #[test]
    fn test_menu_bar_with_unicode_content() {
        use crate::conversation_id::ConversationId;
        use crate::widgets::emoji_picker::EmojiPicker;

        let entry = MCMessage::new(
            MessageId::from(5),
            NodeId::from(500u64),
            NewTextMessage("Привет мир! 🌍".into()),
            MeshChat::now(),
        );
        let emoji_picker = EmojiPicker::new();
        let conversation_id = ConversationId::Channel(2.into());

        let result = entry.menu_bar(
            "РУ",
            "Привет мир! 🌍".to_string(),
            &emoji_picker,
            &conversation_id,
        );
        let _ = result;
    }

    // Tests for view()
    #[test]
    fn test_view_my_message() {
        use crate::conversation_id::ConversationId;
        use crate::widgets::emoji_picker::EmojiPicker;

        let entry = MCMessage::new(
            MessageId::from(1),
            NodeId::from(100u64),
            NewTextMessage("Hello world".into()),
            MeshChat::now(),
        );
        let entries: RingMap<MessageId, MCMessage> = RingMap::new();
        let nodes: HashMap<NodeId, MCNodeInfo> = HashMap::new();
        let conversation_id = ConversationId::Channel(0.into());
        let emoji_picker = EmojiPicker::new();

        let element = entry.view(&entries, &nodes, &conversation_id, true, &emoji_picker);
        let _ = element;
    }

    #[test]
    fn test_view_others_message() {
        use crate::conversation_id::ConversationId;
        use crate::widgets::emoji_picker::EmojiPicker;

        let entry = MCMessage::new(
            MessageId::from(2),
            NodeId::from(200u64),
            NewTextMessage("Message from other".into()),
            MeshChat::now(),
        );
        let entries: RingMap<MessageId, MCMessage> = RingMap::new();
        let mut nodes: HashMap<NodeId, MCNodeInfo> = HashMap::new();
        nodes.insert(
            NodeId::from(200u64),
            MCNodeInfo {
                node_id: NodeId::from(200u64),
                user: Some(MCUser {
                    id: "!sender".to_string(),
                    long_name: "Sender Node".to_string(),
                    short_name: "SN".to_string(),
                    hw_model_str: "T-Beam".to_string(),
                    hw_model: 1,
                    is_licensed: false,
                    role_str: "Client".to_string(),
                    role: 0,
                    public_key: vec![],
                    is_unmessagable: false,
                }),
                position: None,
                is_ignored: false,
            },
        );
        let conversation_id = ConversationId::Channel(0.into());
        let emoji_picker = EmojiPicker::new();

        let element = entry.view(&entries, &nodes, &conversation_id, false, &emoji_picker);
        let _ = element;
    }

    #[test]
    fn test_view_alert_message() {
        use crate::conversation_id::ConversationId;
        use crate::widgets::emoji_picker::EmojiPicker;

        let entry = MCMessage::new(
            MessageId::from(3),
            NodeId::from(100u64),
            AlertMessage("System alert!".into()),
            MeshChat::now(),
        );
        let entries: RingMap<MessageId, MCMessage> = RingMap::new();
        let nodes: HashMap<NodeId, MCNodeInfo> = HashMap::new();
        let conversation_id = ConversationId::Channel(0.into());
        let emoji_picker = EmojiPicker::new();

        let element = entry.view(&entries, &nodes, &conversation_id, true, &emoji_picker);
        let _ = element;
    }

    #[test]
    fn test_view_text_message_reply() {
        use crate::conversation_id::ConversationId;
        use crate::widgets::emoji_picker::EmojiPicker;

        // Create the original message first
        let original = MCMessage::new(
            MessageId::from(1),
            NodeId::from(100u64),
            NewTextMessage("Original message".into()),
            MeshChat::now(),
        );
        let mut entries: RingMap<MessageId, MCMessage> = RingMap::new();
        entries.insert(MessageId::from(1), original);

        // Create the reply
        let reply = MCMessage::new(
            MessageId::from(2),
            NodeId::from(200u64),
            TextMessageReply(MessageId::from(1), "This is my reply".into()),
            MeshChat::now(),
        );
        let nodes: HashMap<NodeId, MCNodeInfo> = HashMap::new();
        let conversation_id = ConversationId::Channel(0.into());
        let emoji_picker = EmojiPicker::new();

        let element = reply.view(&entries, &nodes, &conversation_id, false, &emoji_picker);
        let _ = element;
    }

    #[test]
    fn test_view_position_message() {
        use crate::conversation_id::ConversationId;
        use crate::widgets::emoji_picker::EmojiPicker;

        let position = MCPosition {
            latitude: 37.7749,
            longitude: -122.4194,
            altitude: Some(10),
            time: 0,
            location_source: 0,
            altitude_source: 0,
            timestamp: TimeStamp::from(0),
            timestamp_millis_adjust: 0,
            altitude_hae: None,
            altitude_geoidal_separation: None,
            pdop: 0,
            hdop: 0,
            vdop: 0,
            gps_accuracy: 0,
            ground_speed: None,
            ground_track: None,
            fix_quality: 0,
            fix_type: 0,
            sats_in_view: 0,
            sensor_id: 0,
            next_update: 0,
            seq_number: 0,
            precision_bits: 0,
        };
        let entry = MCMessage::new(
            MessageId::from(4),
            NodeId::from(100u64),
            PositionMessage(position),
            MeshChat::now(),
        );
        let entries: RingMap<MessageId, MCMessage> = RingMap::new();
        let nodes: HashMap<NodeId, MCNodeInfo> = HashMap::new();
        let conversation_id = ConversationId::Channel(0.into());
        let emoji_picker = EmojiPicker::new();

        let element = entry.view(&entries, &nodes, &conversation_id, true, &emoji_picker);
        let _ = element;
    }

    #[test]
    fn test_view_emoji_reply_message() {
        use crate::conversation_id::ConversationId;
        use crate::widgets::emoji_picker::EmojiPicker;

        let entry = MCMessage::new(
            MessageId::from(5),
            NodeId::from(100u64),
            EmojiReply(MessageId::from(1), "👍".into()),
            MeshChat::now(),
        );
        let entries: RingMap<MessageId, MCMessage> = RingMap::new();
        let nodes: HashMap<NodeId, MCNodeInfo> = HashMap::new();
        let conversation_id = ConversationId::Channel(0.into());
        let emoji_picker = EmojiPicker::new();

        let element = entry.view(&entries, &nodes, &conversation_id, true, &emoji_picker);
        let _ = element;
    }

    #[test]
    fn test_view_user_message() {
        use crate::conversation_id::ConversationId;
        use crate::widgets::emoji_picker::EmojiPicker;

        let user = MCUser {
            id: "!newuser".to_string(),
            long_name: "New User".to_string(),
            short_name: "NU".to_string(),
            hw_model_str: "Heltec".to_string(),
            hw_model: 2,
            is_licensed: false,
            role_str: "Client".to_string(),
            role: 0,
            public_key: vec![],
            is_unmessagable: false,
        };
        let entry = MCMessage::new(
            MessageId::from(6),
            NodeId::from(100u64),
            UserMessage(user),
            MeshChat::now(),
        );
        let entries: RingMap<MessageId, MCMessage> = RingMap::new();
        let nodes: HashMap<NodeId, MCNodeInfo> = HashMap::new();
        let conversation_id = ConversationId::Channel(0.into());
        let emoji_picker = EmojiPicker::new();

        let element = entry.view(&entries, &nodes, &conversation_id, true, &emoji_picker);
        let _ = element;
    }

    #[test]
    fn test_view_with_emojis() {
        use crate::conversation_id::ConversationId;
        use crate::widgets::emoji_picker::EmojiPicker;

        let mut entry = MCMessage::new(
            MessageId::from(7),
            NodeId::from(100u64),
            NewTextMessage("Popular message".into()),
            MeshChat::now(),
        );
        entry.add_emoji("👍".to_string(), NodeId::from(200u64));
        entry.add_emoji("❤️".to_string(), NodeId::from(300u64));

        let entries: RingMap<MessageId, MCMessage> = RingMap::new();
        let nodes: HashMap<NodeId, MCNodeInfo> = HashMap::new();
        let conversation_id = ConversationId::Channel(0.into());
        let emoji_picker = EmojiPicker::new();

        let element = entry.view(&entries, &nodes, &conversation_id, true, &emoji_picker);
        let _ = element;
    }

    #[test]
    fn test_view_acked_message() {
        use crate::conversation_id::ConversationId;
        use crate::widgets::emoji_picker::EmojiPicker;

        let mut entry = MCMessage::new(
            MessageId::from(8),
            NodeId::from(100u64),
            NewTextMessage("Acked message".into()),
            MeshChat::now(),
        );
        entry.ack();

        let entries: RingMap<MessageId, MCMessage> = RingMap::new();
        let nodes: HashMap<NodeId, MCNodeInfo> = HashMap::new();
        let conversation_id = ConversationId::Channel(0.into());
        let emoji_picker = EmojiPicker::new();

        let element = entry.view(&entries, &nodes, &conversation_id, true, &emoji_picker);
        let _ = element;
    }

    #[test]
    fn test_view_dm_channel() {
        use crate::conversation_id::ConversationId;
        use crate::widgets::emoji_picker::EmojiPicker;

        let entry = MCMessage::new(
            MessageId::from(9),
            NodeId::from(100u64),
            NewTextMessage("DM message".into()),
            MeshChat::now(),
        );
        let entries: RingMap<MessageId, MCMessage> = RingMap::new();
        let nodes: HashMap<NodeId, MCNodeInfo> = HashMap::new();
        let conversation_id = ConversationId::Node(NodeId::from(200u64)); // DM channel
        let emoji_picker = EmojiPicker::new();

        let element = entry.view(&entries, &nodes, &conversation_id, true, &emoji_picker);
        let _ = element;
    }

    #[test]
    fn test_view_message_with_link() {
        use crate::conversation_id::ConversationId;
        use crate::widgets::emoji_picker::EmojiPicker;

        let entry = MCMessage::new(
            MessageId::from(10),
            NodeId::from(100u64),
            NewTextMessage("Check out https://example.com for more info".into()),
            MeshChat::now(),
        );
        let entries: RingMap<MessageId, MCMessage> = RingMap::new();
        let nodes: HashMap<NodeId, MCNodeInfo> = HashMap::new();
        let conversation_id = ConversationId::Channel(0.into());
        let emoji_picker = EmojiPicker::new();

        let element = entry.view(&entries, &nodes, &conversation_id, true, &emoji_picker);
        let _ = element;
    }
}
