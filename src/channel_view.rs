use crate::Message::DeviceViewEvent;
use crate::channel_view::ChannelId::{Channel, Node};
use crate::channel_view::ChannelViewMessage::{ClearMessage, MessageInput, SendMessage};
use crate::channel_view_entry::Payload::{
    EmojiReply, NewTextMessage, PositionMessage, TextMessageReply, UserMessage,
};
use crate::device_view::DeviceViewMessage;
use crate::device_view::DeviceViewMessage::{ChannelMsg, SendInfoMessage, SendPositionMessage};
use crate::styles::{DAY_SEPARATOR_STYLE, button_chip_style, text_input_style};
use crate::{Message, channel_view_entry::ChannelViewEntry, icons};
use chrono::prelude::DateTime;
use chrono::{Datelike, Local};
use iced::padding::right;
use iced::widget::scrollable::Scrollbar;
use iced::widget::text::Shaping::Advanced;
use iced::widget::text_input::{Icon, Side};
use iced::widget::{Column, Container, Row, Space, button, scrollable, text, text_input};
use iced::{Center, Element, Fill, Font, Padding, Pixels, Task};
use meshtastic::packet::PacketDestination;
use meshtastic::types::{MeshChannel, NodeId};
use ringmap::RingMap;
use serde::{Deserialize, Serialize};
use std::fmt::{Display, Formatter};
use std::hash::Hash;

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

impl ChannelId {
    pub fn to_destination(&self) -> (PacketDestination, MeshChannel) {
        match self {
            Channel(channel_number) => (
                PacketDestination::Broadcast,
                MeshChannel::from(*channel_number as u32),
            ),
            Node(node_id) => (
                PacketDestination::Node(NodeId::from(*node_id)),
                MeshChannel::default(),
            ),
        }
    }
}

/// [ChannelView] implements view and update methods for Iced for a set of
/// messages to and from a "Channel" which can be a Channel or a Node
pub struct ChannelView {
    channel_id: ChannelId,
    message: String,                         // text message typed in so far
    entries: RingMap<u32, ChannelViewEntry>, // entries received so far, keyed by message_id, ordered by rx_time
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
            entries: RingMap::new(),
            my_source: source,
        }
    }

    /// Acknowledge the receipt of a message.
    pub fn ack(&mut self, request_id: u32) {
        if let Some(entry) = self.entries.get_mut(&request_id) {
            entry.ack();
        }
    }

    /// Add an emoji reply to a message.
    fn add_emoji_to(&mut self, request_id: u32, emoji_string: String, source_name: String) {
        if let Some(entry) = self.entries.get_mut(&request_id) {
            entry.add_emoji(emoji_string, source_name);
        }
    }

    /// Add a new [ChannelViewEntry] message to the [ChannelView]
    pub fn new_message(&mut self, new_message: ChannelViewEntry) {
        match &new_message.payload() {
            NewTextMessage(_) | PositionMessage(_, _) | UserMessage(_) | TextMessageReply(_, _) => {
                // TODO manage the size of entries, with a limit (fixed or time?), and pushing
                // the older ones to a disk store of messages
                self.entries.insert_sorted_by(
                    new_message.message_id(),
                    new_message,
                    ChannelViewEntry::sort_by_rx_time,
                );
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

    /// Return the number of messages in the channel
    pub fn num_unseen_messages(&self) -> usize {
        self.entries.len()
    }

    /// Update the [ChannelView] state based on a [ChannelViewMessage]
    pub fn update(&mut self, channel_view_message: ChannelViewMessage) -> Task<Message> {
        match channel_view_message {
            MessageInput(s) => {
                if s.len() < 200 {
                    self.message = s;
                }
                Task::none()
            }
            ClearMessage => {
                self.message = String::new();
                Task::none()
            }
            SendMessage => {
                if !self.message.is_empty() {
                    let msg = self.message.clone();
                    self.message = String::new();
                    let channel_id = self.channel_id.clone();
                    Task::perform(empty(), move |_| {
                        DeviceViewEvent(DeviceViewMessage::SendTextMessage(
                            msg.clone(),
                            channel_id.clone(),
                        ))
                    })
                } else {
                    Task::none()
                }
            }
        }
    }

    /// Construct an Element that displays the channel view
    pub fn view(&self, enable_position: bool) -> Element<'_, Message> {
        let mut channel_view = Column::new().padding(right(10));

        let mut previous_day = u32::MIN;

        // Add a view to the column for each of the entries in this Channel
        for entry in self.entries.values() {
            let message_day = entry.time().day();

            // Add a day separator when the day of an entry changes
            if message_day != previous_day {
                channel_view = channel_view.push(Self::day_separator(&entry.time()));
                previous_day = message_day;
            }

            channel_view =
                channel_view.push(entry.view(&self.entries, entry.source_node(self.my_source)));
        }

        // Wrap the list of messages in a scrollable container, with a scrollbar
        let channel_scroll = scrollable(channel_view)
            .direction({
                let scrollbar = Scrollbar::new().width(10.0);
                scrollable::Direction::Vertical(scrollbar)
            })
            .width(Fill)
            .height(Fill);

        // A row of action buttons at the bottom of the channel view - this could be made
        // a menu or something different in the future
        let mut send_position_button =
            button(text("Send Position 📌").shaping(Advanced)).style(button_chip_style);
        if enable_position {
            send_position_button = send_position_button.on_press(DeviceViewEvent(
                SendPositionMessage(self.channel_id.clone()),
            ));
        }
        let channel_buttons = Row::new()
            .push(send_position_button)
            .push(Space::with_width(6))
            .push(
                button(text("Send Info"))
                    .style(button_chip_style)
                    .on_press(DeviceViewEvent(SendInfoMessage(self.channel_id.clone()))),
            );

        // Place the scrollable in a column, with an input box at the bottom
        Column::new()
            .padding(4)
            .push(channel_scroll)
            .push(channel_buttons)
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
        let today = now_local.day();

        let format_string = if datetime_local.iso_week() < now_local.iso_week() {
            if datetime_local.year() != now_local.year() {
                // before the beginning of year week, so how the day name, month name and date
                "%A, %b %e, %Y"
            } else {
                // before the beginning of this week, so how the day name, month name and date
                "%A, %b %e"
            }
        } else if datetime_local.day() == today {
            "Today"
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

    fn input_box(&self) -> Element<'static, Message> {
        let mut send_button = button(icons::send().size(18))
            .style(button_chip_style)
            .padding(Padding::from([6, 6]));
        let mut clear_button = button(text("X").size(18))
            .style(button_chip_style)
            .padding(Padding::from([6, 6]));
        if !self.message.is_empty() {
            send_button = send_button.on_press(DeviceViewEvent(ChannelMsg(SendMessage)));
            clear_button = clear_button.on_press(DeviceViewEvent(ChannelMsg(ClearMessage)));
        }

        Row::new()
            .align_y(Center)
            .push(
                text_input("Send Message", &self.message)
                    .style(text_input_style)
                    .on_input(|s| DeviceViewEvent(ChannelMsg(MessageInput(s))))
                    .on_submit(DeviceViewEvent(ChannelMsg(SendMessage)))
                    .padding([6, 6])
                    .icon(Icon {
                        font: Font::with_name("icons"),
                        code_point: '\u{2709}',
                        size: Some(Pixels(18.0)),
                        spacing: 4.0,
                        side: Side::Left,
                    }),
            )
            .push(Space::with_width(4.0))
            .push(clear_button)
            .push(Space::with_width(4.0))
            .push(send_button)
            .into()
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
        let mut iter = channel_view.entries.values();
        assert_eq!(iter.next().unwrap(), &oldest_message);
        assert_eq!(iter.next().unwrap(), &middle_message);
        assert_eq!(iter.next().unwrap(), &newest_message);
    }
}
