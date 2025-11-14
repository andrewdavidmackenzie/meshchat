use crate::channel_view::ChannelId::Channel;
use crate::channel_view::ChannelViewMessage::{ClearMessage, MessageInput};
use crate::channel_view_entry::Payload::{Ping, Position, TextMessage};
use crate::device_view::DeviceViewMessage::ChannelMsg;
use crate::styles::{
    text_input_style, DAY_SEPARATOR_STYLE, MESSAGE_TEXT_STYLE, MY_MESSAGE_BUBBLE_STYLE,
    OTHERS_MESSAGE_BUBBLE_STYLE, TIME_TEXT_COLOR, TIME_TEXT_SIZE, TIME_TEXT_WIDTH,
};
use crate::{channel_view_entry::ChannelViewEntry, Message};
use chrono::prelude::DateTime;
use chrono::{Datelike, Local, Utc};
use iced::widget::scrollable::Scrollbar;
use iced::widget::{scrollable, text, text_input, Column, Container, Row, Space, Text};
use iced::Length::Fixed;
use iced::{Bottom, Center, Element, Fill, Left, Renderer, Right, Task, Theme};
use serde::{Deserialize, Serialize};
use sorted_vec::SortedVec;
use std::fmt::{Display, Formatter};

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

    /// WHen a message was sent, add it to the list of messages to display with the current time
    pub fn message_sent(&mut self, msg_text: String) {
        self.entries.push(ChannelViewEntry::new(
            TextMessage(msg_text),
            self.my_source,
            true,
        ));
        // Until we have a queue of messages being sent pending confirmation
        self.message = String::new();
    }

    pub fn new_message(&mut self, new_message: ChannelViewEntry) {
        self.entries.push(new_message);
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
                    Message::Device(crate::device_view::DeviceViewMessage::SendMessage(
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
                    .padding(6)
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
            .on_input(|s| Message::Device(ChannelMsg(MessageInput(s))))
            .on_submit(Message::Device(ChannelMsg(ChannelViewMessage::SendMessage)))
            .padding([6, 6])
            .into()
    }

    /// Create an Element that contains a message received or sent
    fn message_box(&self, message: &ChannelViewEntry) -> Element<'static, Message> {
        let style = if message.source_node(self.my_source) {
            MY_MESSAGE_BUBBLE_STYLE
        } else {
            OTHERS_MESSAGE_BUBBLE_STYLE
        };

        // TODO in the future we might change graphics based on type - just text for now
        let msg = match message.payload() {
            TextMessage(text_msg) => text_msg.clone(),
            Position(lat, long) => {
                let latitude = 0.0000001 * *lat as f64;
                let longitude = 0.0000001 * *long as f64;
                format!("({}, {})", latitude, longitude)
            }
            Ping(short_name) => format!("Ping from user '{}'", short_name),
        };

        let bubble = Container::new(
            Row::new()
                .push(
                    text(msg)
                        .style(|_| MESSAGE_TEXT_STYLE)
                        .size(18)
                        .shaping(text::Shaping::Advanced),
                )
                .push(Space::with_width(10.0))
                .push(Self::time_to_text(message.time()))
                .align_y(Bottom),
        )
        .padding([6, 12])
        .style(move |_theme: &Theme| style);

        let mut row = Row::new().padding([6, 6]);
        // Put on the right hand side if my message, on the left if from someone else
        if message.source_node(self.my_source) {
            // Avoid very wide messages from me extending all the way to the left edge of the screen
            row = row.push(Space::with_width(100.0)).push(bubble);
            Column::new().width(Fill).align_x(Right).push(row).into()
        } else {
            // TODO from - maybe a name or an icon from the u32? Need to store them somewhere?
            // Avoid very wide messages from others extending all the way to the right edge
            row = row.push(bubble).push(Space::with_width(100.0));
            Column::new().width(Fill).align_x(Left).push(row).into()
        }
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
