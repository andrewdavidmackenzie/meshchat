use crate::Message::DeviceViewEvent;
use crate::channel_view::ChannelId::{Channel, Node};
use crate::channel_view::ChannelViewMessage::{
    CancelPrepareReply, ClearMessage, MessageInput, PrepareReply, SendMessage,
};
use crate::channel_view_entry::Payload::{
    EmojiReply, NewTextMessage, PositionMessage, TextMessageReply, UserMessage,
};
use crate::device_view::DeviceViewMessage;
use crate::device_view::DeviceViewMessage::{ChannelMsg, SendInfoMessage, SendPositionMessage};
use crate::styles::{DAY_SEPARATOR_STYLE, button_chip_style, reply_to_style, text_input_style};
use crate::{Message, channel_view_entry::ChannelViewEntry, icons};
use chrono::prelude::DateTime;
use chrono::{Datelike, Local};
use iced::font::Style::Italic;
use iced::padding::right;
use iced::widget::scrollable::Scrollbar;
use iced::widget::text::Shaping::Advanced;
use iced::widget::text_input::{Icon, Side};
use iced::widget::{
    Button, Column, Container, Row, Space, button, container, scrollable, text, text_input,
};
use iced::{Center, Element, Fill, Font, Padding, Pixels, Task};
use meshtastic::packet::PacketDestination;
use meshtastic::protobufs::NodeInfo;
use meshtastic::types::{MeshChannel, NodeId};
use ringmap::RingMap;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt::{Display, Formatter};
use std::hash::Hash;

#[derive(Debug, Clone)]
pub enum ChannelViewMessage {
    MessageInput(String),
    ClearMessage,
    SendMessage(Option<u32>), // optional message id if we are replying to that message
    PrepareReply(u32),        // entry_id
    CancelPrepareReply,
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
    preparing_reply: Option<u32>,
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
            preparing_reply: None,
        }
    }

    /// Acknowledge the receipt of a message.
    pub fn ack(&mut self, request_id: u32) {
        if let Some(entry) = self.entries.get_mut(&request_id) {
            entry.ack();
        }
    }

    /// Add an emoji reply to a message.
    fn add_emoji_to(&mut self, request_id: u32, emoji_string: String, from: u32) {
        if let Some(entry) = self.entries.get_mut(&request_id) {
            entry.add_emoji(emoji_string, from);
        }
    }

    /// Add a new [ChannelViewEntry] message to the [ChannelView]
    pub fn new_message(&mut self, new_message: ChannelViewEntry) {
        match &new_message.payload() {
            NewTextMessage(_) | PositionMessage(_, _) | UserMessage(_) | TextMessageReply(_, _) => {
                self.entries.insert_sorted_by(
                    new_message.message_id(),
                    new_message,
                    ChannelViewEntry::sort_by_rx_time,
                );
            }
            EmojiReply(reply_to_id, emoji_string) => {
                self.add_emoji_to(*reply_to_id, emoji_string.clone(), new_message.from());
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
            SendMessage(reply_to_id) => {
                if !self.message.is_empty() {
                    let msg = self.message.clone();
                    self.message = String::new();
                    let channel_id = self.channel_id.clone();
                    self.preparing_reply = None;
                    Task::perform(empty(), move |_| {
                        DeviceViewEvent(DeviceViewMessage::SendTextMessage(
                            msg.clone(),
                            channel_id.clone(),
                            reply_to_id,
                        ))
                    })
                } else {
                    Task::none()
                }
            }
            PrepareReply(entry_id) => {
                self.preparing_reply = Some(entry_id);
                Task::none()
            }
            CancelPrepareReply => {
                self.preparing_reply = None;
                Task::none()
            }
        }
    }

    /// Construct an Element that displays the channel view
    pub fn view<'a>(
        &'a self,
        nodes: &'a HashMap<u32, NodeInfo>,
        enable_position: bool,
        enable_my_info: bool,
    ) -> Element<'a, Message> {
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

            if let Some(element) =
                entry.view(&self.entries, nodes, entry.source_node(self.my_source))
            {
                channel_view = channel_view.push(element);
            }
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

        let mut send_info_button =
            button(text("Send Info ⓘ").shaping(Advanced)).style(button_chip_style);
        if enable_my_info {
            send_info_button = send_info_button
                .on_press(DeviceViewEvent(SendInfoMessage(self.channel_id.clone())));
        }

        let channel_buttons = Row::new()
            .padding([2, 0])
            .push(send_position_button)
            .push(Space::with_width(6))
            .push(send_info_button);

        // Place the scrollable in a column, with a row of buttons at the bottom
        let mut column = Column::new()
            .padding(4)
            .push(channel_scroll)
            .push(channel_buttons);

        // If we are replying to a message, add a row at the bottom of the channel view with the original text
        if let Some(entry_id) = &self.preparing_reply {
            column = self.replying_to(column, entry_id);
        }

        // Add the input box at the bottom of the channel view
        column.push(self.input_box()).into()
    }

    /// Add a row that explains we are replying to a prior message
    fn replying_to<'a>(
        &'a self,
        column: Column<'a, Message>,
        entry_id: &u32,
    ) -> Column<'a, Message> {
        let original_text = match self.entries.get(entry_id).unwrap().payload() {
            NewTextMessage(original_text) => original_text.clone(),
            TextMessageReply(_, original_text) => original_text.clone(),
            EmojiReply(_, original_text) => original_text.clone(),
            PositionMessage(lat, lon) => format!("📌 ({:.2}, {:.2})", lat, lon),
            UserMessage(user) => format!(
                "Reply to ⓘ message from {} ({})",
                user.long_name.clone(),
                user.short_name.clone()
            ),
        };
        let cancel_reply_button: Button<Message> = button(text("⨂").shaping(Advanced).size(16))
            .on_press(DeviceViewEvent(ChannelMsg(CancelPrepareReply)))
            .style(button_chip_style)
            .padding(0);

        column.push(
            container(
                Row::new()
                    .align_y(Center)
                    .padding(2)
                    .push(Space::with_width(24))
                    .push(
                        text(format!("Replying to: '{}'", original_text))
                            .shaping(Advanced)
                            .font(Font {
                                style: Italic,
                                ..Default::default()
                            }),
                    )
                    .push(Space::with_width(8))
                    .push(cancel_reply_button),
            )
            .width(Fill)
            .style(reply_to_style),
        )
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
        let mut clear_button = button(text("⨂").shaping(Advanced).size(18))
            .style(button_chip_style)
            .padding(Padding::from([6, 6]));
        if !self.message.is_empty() {
            send_button = send_button.on_press(DeviceViewEvent(ChannelMsg(SendMessage(
                self.preparing_reply,
            ))));
            clear_button = clear_button.on_press(DeviceViewEvent(ChannelMsg(ClearMessage)));
        }

        Row::new()
            .align_y(Center)
            .push(
                text_input("Send Message", &self.message)
                    .style(text_input_style)
                    .on_input(|s| DeviceViewEvent(ChannelMsg(MessageInput(s))))
                    .on_submit(DeviceViewEvent(ChannelMsg(SendMessage(
                        self.preparing_reply,
                    ))))
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
        let oldest_message =
            ChannelViewEntry::new(NewTextMessage("Hello 1".to_string()), 1, 1, false);
        tokio::time::sleep(Duration::from_millis(1500)).await;

        let middle_message =
            ChannelViewEntry::new(NewTextMessage("Hello 2".to_string()), 2, 1000, false);
        tokio::time::sleep(Duration::from_millis(1500)).await;

        let newest_message =
            ChannelViewEntry::new(NewTextMessage("Hello 3".to_string()), 1, 500, false);

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
