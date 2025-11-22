use crate::Message::{Device, ShowLocation};
use crate::channel_view::ChannelId::Channel;
use crate::channel_view::ChannelViewMessage::{ClearMessage, MessageInput};
use crate::channel_view_entry::Payload::{
    EmojiReply, NewTextMessage, Ping, Position, TextMessageReply,
};
use crate::device_view::DeviceViewMessage::{ChannelMsg, ShowChannel};
use crate::styles::{
    COLOR_DICTIONARY, COLOR_GREEN, DAY_SEPARATOR_STYLE, MESSAGE_TEXT_STYLE,
    MY_MESSAGE_BUBBLE_STYLE, OTHERS_MESSAGE_BUBBLE_STYLE, TIME_TEXT_COLOR, TIME_TEXT_SIZE,
    TIME_TEXT_WIDTH, source_tooltip_style, text_input_style, transparent_button_style,
};
use crate::{Message, channel_view_entry::ChannelViewEntry};
use chrono::prelude::DateTime;
use chrono::{Datelike, Local, Utc};
use iced::Length::Fixed;
use iced::font::Weight;
use iced::widget::scrollable::Scrollbar;
use iced::widget::text::Shaping::Advanced;
use iced::widget::{
    Column, Container, Row, Space, Text, button, scrollable, text, text_input, tooltip,
};
use iced::{
    Bottom, Center, Color, Element, Fill, Font, Left, Padding, Renderer, Right, Task, Theme,
};
use serde::{Deserialize, Serialize};
use sorted_vec::SortedVec;
use std::collections::hash_map::DefaultHasher;
use std::fmt::{Display, Formatter};
use std::hash::{Hash, Hasher};
use std::mem;

#[derive(Debug, Clone)]
pub enum ChannelViewMessage {
    MessageInput(String),
    ClearMessage,
    SendMessage,
}

#[derive(Debug, Clone, Serialize, Deserialize, Hash, PartialEq, Eq)]
pub enum ChannelId {
    Channel(i32), // Channel::index 0..7
    Node(u32),    // NodeInfo::node number
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

/// [ChannelView] implements view and update methods for Iced for a set of
/// messages to and from a "Channel" which can be a Channel or a Node
pub struct ChannelView {
    channel_id: ChannelId,
    message: String,                      // text message typed in so far
    entries: SortedVec<ChannelViewEntry>, // entries received so far
    my_source: u32,
}

async fn empty() {}

// A view of a single channel and it's message, which maybe a real radio "Channel" or a chat channel
// with a specific [meshtastic:User]
impl ChannelView {
    pub fn new(channel_id: ChannelId, source: u32) -> Self {
        Self {
            channel_id,
            message: String::new(),
            entries: SortedVec::new(),
            my_source: source,
        }
    }

    /// Acknowledge the receipt of a message.
    pub fn ack(&mut self, request_id: u32) {
        // Convert to Vec to allow mutable access, modify entries, then rebuild SortedVec
        let mut entries_vec: Vec<ChannelViewEntry> =
            mem::take(&mut self.entries).into_iter().collect();
        for entry in entries_vec.iter_mut() {
            if entry.message_id() == request_id {
                entry.ack();
                break;
            }
        }
        self.entries = SortedVec::from_unsorted(entries_vec);
    }

    fn add_emoji_to(&mut self, request_id: u32, emoji_string: String, source_name: String) {
        // Convert to Vec to allow mutable access, modify entries, then rebuild SortedVec
        let mut entries_vec: Vec<ChannelViewEntry> =
            mem::take(&mut self.entries).into_iter().collect();
        for entry in entries_vec.iter_mut() {
            if entry.message_id() == request_id {
                entry.add_emoji(emoji_string, source_name);
                break;
            }
        }
        self.entries = SortedVec::from_unsorted(entries_vec);
    }

    /// Return the text of a NewTextMessage that matches the given id, or None if not found
    fn text_from_id(&self, id: u32) -> Option<String> {
        for entry in &self.entries {
            if let NewTextMessage(text_message) = entry.payload()
                && entry.message_id() == id
            {
                return Some(text_message.clone());
            }
        }
        None
    }

    pub fn new_message(&mut self, new_message: ChannelViewEntry) {
        match &new_message.payload() {
            NewTextMessage(_) | Position(_, _) | Ping(_) | TextMessageReply(_, _) => {
                // TODO manage the size of entries, with a limit (fixed or time?), and pushing
                // the older ones to a disk store of messages
                let _ = self.entries.push(new_message);
            }
            EmojiReply(reply_to_id, emoji_string) => {
                self.add_emoji_to(
                    *reply_to_id,
                    emoji_string.clone(),
                    new_message.name().clone().unwrap_or("".to_string()),
                );
            }
        };
    }

    pub fn num_unseen_messages(&self) -> usize {
        self.entries.len()
    }

    pub fn update(&mut self, channel_view_message: ChannelViewMessage) -> Task<Message> {
        match channel_view_message {
            MessageInput(s) => {
                self.message = s;
                Task::none()
            }
            ClearMessage => {
                // TODO add a button to clear message entered so far
                self.message = String::new();
                Task::none()
            }
            ChannelViewMessage::SendMessage => {
                let msg = self.message.clone();
                self.message = String::new();
                let channel_id = self.channel_id.clone();
                Task::perform(empty(), move |_| {
                    Device(crate::device_view::DeviceViewMessage::SendMessage(
                        msg.clone(),
                        channel_id.clone(),
                    ))
                })
            }
        }
    }

    // Make this a struct and move the message field in here
    pub fn view(&self) -> Element<'static, Message> {
        let mut channel_view = Column::new();

        let mut previous_day = u32::MIN;

        for message in &self.entries {
            let datetime_utc = DateTime::<Utc>::from_timestamp_secs(message.time() as i64).unwrap();
            let datetime_local = datetime_utc.with_timezone(&Local);
            let message_day = datetime_local.day();

            if message_day != previous_day {
                channel_view = channel_view.push(Self::day_separator(&datetime_local));
                previous_day = message_day;
            }

            channel_view = channel_view.push(self.message_box(message));
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

    /// Return an Element that displays a day separator
    /// If in the same week, then just the day name "Friday"
    /// If in a previous week, of the same year, then "Friday, Jul 8"
    /// If in a previous year, then "Friday, Jul 8, 2021"
    ///
    /// Meaning of format strings used
    /// "%Y" - Year . e.g. "2021"
    /// "%b" - Three letter month name e.g. "Jul"
    /// "%e" - space padded date e.g. " 8" for the 8th day of the month
    /// "%A" - day name e.g. "Friday"
    fn day_separator(datetime_local: &DateTime<Local>) -> Element<'static, Message> {
        let now_local = Local::now();
        let format_string = if datetime_local.iso_week() < now_local.iso_week() {
            if datetime_local.year() != now_local.year() {
                // before the beginning of year week, so how the day name, month name and date
                "%A, %b %e, %Y"
            } else {
                // before the beginning of this week, so how the day name, month name and date
                "%A, %b %e"
            }
        } else {
            // Same week, so just show the day name
            "%A"
        };

        Column::new()
            .push(
                Container::new(text(datetime_local.format(format_string).to_string()).size(16))
                    .align_x(Center)
                    .padding(Padding::from([6, 12]))
                    .style(|_| DAY_SEPARATOR_STYLE),
            )
            .width(Fill)
            .align_x(Center)
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
            .on_input(|s| Device(ChannelMsg(MessageInput(s))))
            .on_submit(Device(ChannelMsg(ChannelViewMessage::SendMessage)))
            .padding([6, 6])
            .into()
    }

    /// Create an Element that contains a message received or sent
    fn message_box(&self, message: &ChannelViewEntry) -> Element<'static, Message> {
        let mine = message.source_node(self.my_source);
        let style = if mine {
            MY_MESSAGE_BUBBLE_STYLE
        } else {
            OTHERS_MESSAGE_BUBBLE_STYLE
        };

        // Add the source node name if there is one
        // TODO try and show the full node name in the tooltip
        let mut message_content_column = Column::new();
        if let Some(name) = message.name() {
            let text_color = Self::color_from_name(name);
            message_content_column = message_content_column.push(
                tooltip(
                    button(text(name.clone()).shaping(Advanced).font(Font {
                        weight: Weight::Bold,
                        ..Default::default()
                    }))
                    .padding(0)
                    .on_press(Device(ShowChannel(Some(ChannelId::Node(message.from())))))
                    .style(move |theme, status| {
                        transparent_button_style(theme, status, text_color)
                    }),
                    text(format!("Click name to DM node '{name}'")).shaping(Advanced),
                    tooltip::Position::Top,
                )
                .style(source_tooltip_style),
            );
        }

        // Add a row to the message we are replying to if there is one
        if let TextMessageReply(reply_to_id, _) = message.payload()
            && let Some(original_text) = self.text_from_id(*reply_to_id)
        {
            let quote_row = Row::new()
                .push(text("Re: ").color(COLOR_GREEN).shaping(Advanced))
                .push(text(original_text).color(COLOR_GREEN).shaping(Advanced));
            message_content_column = message_content_column.push(quote_row);
        };

        // TODO in the future we might change graphics based on type - just text for now
        let content: Element<'static, Message> = match message.payload() {
            NewTextMessage(text_msg) | TextMessageReply(_, text_msg) => text(text_msg.clone())
                .style(|_| MESSAGE_TEXT_STYLE)
                .size(18)
                .shaping(Advanced)
                .into(),
            Position(lat, long) => {
                let latitude = 0.0000001 * *lat as f64;
                let longitude = 0.0000001 * *long as f64;
                button(text(format!("({:.2}, {:.2}) 📌", latitude, longitude)).shaping(Advanced))
                    .padding(0)
                    .style(|theme, status| transparent_button_style(theme, status, COLOR_GREEN))
                    .on_press(ShowLocation(latitude, longitude))
                    .into()
            }
            Ping(_) => text("Ping!")
                .style(|_| MESSAGE_TEXT_STYLE)
                .size(18)
                .shaping(Advanced)
                .into(),
            EmojiReply(_, _) => text("").into(), // Should never happen
        };

        // Create the row with message text and time and maybe an ACK tick mark
        let mut text_and_time_row = Row::new()
            .push(content)
            .push(Space::with_width(10.0))
            .push(Self::time_to_text(message.time()))
            .align_y(Bottom);

        if message.acked() {
            text_and_time_row = text_and_time_row.push(text("✓").size(14).color(COLOR_GREEN))
        };

        // Add the message text and time row
        message_content_column = message_content_column.push(text_and_time_row);

        // Create the container around the message content column and style it
        let message_bubble = Container::new(message_content_column)
            .padding([6, 8])
            .style(move |_theme: &Theme| style);

        let mut message_row = Row::new().padding([6, 6]);

        // The outer container object that spans the width of the view and justifies the message
        // row within it depending on who sent the message
        let mut message_column = Column::new().width(Fill);

        // Put on the right-hand side if my message, on the left if from someone else
        if mine {
            // Avoid very wide messages from me extending all the way to the left edge of the screen
            message_row = message_row
                .push(Space::with_width(100.0))
                .push(message_bubble);
            message_column = message_column.align_x(Right).push(message_row);
        } else {
            // Avoid very wide messages from others extending all the way to the right edge
            message_row = message_row
                .push(message_bubble)
                .push(Space::with_width(100.0));
            message_column = message_column.align_x(Left).push(message_row);
        };

        // Add the emoji row outside the bubble, below it
        if !message.emojis().is_empty() {
            let mut emoji_row = Row::new().padding([0, 6]);
            // TODO Style the tooltip
            for (emoji, sources) in message.emojis() {
                let tooltip_element: Element<'_, Message> = self.list_of_nodes(sources);
                emoji_row = emoji_row.push(tooltip(
                    text(emoji.clone()).size(18).shaping(Advanced),
                    tooltip_element,
                    tooltip::Position::Bottom,
                ));
            }
            message_column = message_column.push(emoji_row);
        }

        message_column.into()
    }

    /// Hash the node name into a color index - to be able to assign a consistent color to the text
    /// name for a node in the UI
    fn color_from_name(name: &String) -> Color {
        let mut hasher = DefaultHasher::new();
        name.hash(&mut hasher);
        let hash = hasher.finish();

        let index = (hash % COLOR_DICTIONARY.len() as u64) as usize;
        COLOR_DICTIONARY[index]
    }

    /// Return an element (currently a Column) with a list of the names of the nodes that sent the
    /// given emoji.
    fn list_of_nodes(&self, sources: &Vec<String>) -> Element<'static, Message> {
        let mut col = Column::new();
        for source in sources {
            col = col.push(
                text(source.clone())
                    .color(Self::color_from_name(source))
                    .shaping(Advanced),
            );
        }
        col.into()
    }

    /// Format a time as seconds in epoc (u64) into a String of hour and minutes during the day
    /// it occurs in. These will be separated by Day specifiers, so day is not needed.
    fn time_to_text(time: u64) -> Text<'static, Theme, Renderer> {
        let datetime_utc = DateTime::<Utc>::from_timestamp_secs(time as i64).unwrap();
        let datetime_local = datetime_utc.with_timezone(&Local);
        let time_str = datetime_local.format("%H:%M").to_string(); // Formats as HH:MM
        text(time_str)
            .color(TIME_TEXT_COLOR)
            .size(TIME_TEXT_SIZE)
            .width(Fixed(TIME_TEXT_WIDTH))
    }
}

#[cfg(test)]
mod test {
    use crate::channel_view::{ChannelId, ChannelView};
    use crate::channel_view_entry::ChannelViewEntry;
    use crate::channel_view_entry::Payload::NewTextMessage;
    use std::time::Duration;

    #[tokio::test]
    async fn message_ordering_test() {
        let mut channel_view = ChannelView::new(ChannelId::Channel(0), 0);
        assert!(
            channel_view.entries.is_empty(),
            "Initial entries list should be empty"
        );

        // create a set of messages with more than a second between them
        // message ids are not in order
        let oldest_message = ChannelViewEntry::new(
            NewTextMessage("Hello 1".to_string()),
            1,
            1,
            Some("Source 1".to_string()),
            false,
        );
        tokio::time::sleep(Duration::from_millis(1500)).await;

        let middle_message = ChannelViewEntry::new(
            NewTextMessage("Hello 2".to_string()),
            2,
            1000,
            Some("Source 2".to_string()),
            false,
        );
        tokio::time::sleep(Duration::from_millis(1500)).await;

        let newest_message = ChannelViewEntry::new(
            NewTextMessage("Hello 3".to_string()),
            1,
            500,
            Some("Source 1".to_string()),
            false,
        );

        // Add them in order
        channel_view.new_message(oldest_message.clone());
        channel_view.new_message(middle_message.clone());
        channel_view.new_message(newest_message.clone());

        assert_eq!(channel_view.num_unseen_messages(), 3);

        println!("Entries: {:#?}", channel_view.entries);

        // Check the order is correct
        let mut iter = channel_view.entries.iter();
        assert_eq!(iter.next().unwrap(), &oldest_message);
        assert_eq!(iter.next().unwrap(), &middle_message);
        assert_eq!(iter.next().unwrap(), &newest_message);
    }
}
