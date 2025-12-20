use crate::Message::DeviceViewEvent;
use crate::channel_view::ChannelId::{Channel, Node};
use crate::channel_view::ChannelViewMessage::{
    CancelPrepareReply, ClearMessage, MessageInput, MessageSeen, PickChannel, PrepareReply,
    SendMessage,
};
use crate::channel_view_entry::Payload::{
    AlertMessage, EmojiReply, NewTextMessage, PositionMessage, TextMessageReply, UserMessage,
};
use crate::config::Config;
use crate::device_view::DeviceViewMessage::{
    ChannelMsg, ForwardMessage, SendInfoMessage, SendPositionMessage, ShowChannel,
    StopForwardingMessage,
};
use crate::device_view::{DeviceView, DeviceViewMessage};
use crate::styles::{
    DAY_SEPARATOR_STYLE, button_chip_style, picker_header_style, reply_to_style, scrollbar_style,
    text_input_style, tooltip_style,
};
use crate::{Message, channel_view_entry::ChannelViewEntry, icons};
use chrono::prelude::DateTime;
use chrono::{Datelike, Local};
use iced::font::Style::Italic;
use iced::font::Weight;
use iced::padding::right;
use iced::widget::scrollable::Scrollbar;
use iced::widget::text_input::{Icon, Side};
use iced::widget::{
    Button, Column, Container, Row, Space, button, center, container, mouse_area, opaque,
    scrollable, stack, text, text_input,
};
use iced::{Center, Color, Element, Fill, Font, Padding, Pixels, Task};
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
    MessageSeen(ChannelId, u32),
    PickChannel(Option<ChannelId>),
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
#[derive(Debug, Default)]
pub struct ChannelView {
    channel_id: ChannelId,
    message: String,                         // text message typed in so far
    entries: RingMap<u32, ChannelViewEntry>, // entries received so far, keyed by message_id, ordered by rx_time
    source: u32,
    preparing_reply: Option<u32>,
}

async fn empty() {}

// A view of a single channel and it's message, which maybe a real radio "Channel" or a chat channel
// with a specific [meshtastic:User]
impl ChannelView {
    pub fn new(channel_id: ChannelId, source: u32) -> Self {
        Self {
            channel_id,
            source,
            ..Default::default()
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
            AlertMessage(_)
            | NewTextMessage(_)
            | PositionMessage(_, _)
            | UserMessage(_)
            | TextMessageReply(_, _) => {
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
        self.entries
            .values()
            .fold(0, |acc, e| if !e.seen { acc + 1 } else { acc })
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
                self.cancel_interactive();
                Task::none()
            }
            MessageSeen(_, message_id) => {
                if let Some(channel_view_entry) = self.entries.get_mut(&message_id) {
                    channel_view_entry.seen = true;
                }
                Task::none()
            }
            PickChannel(channel_id) => {
                Task::perform(empty(), move |_| DeviceViewEvent(ShowChannel(channel_id)))
            }
        }
    }

    /// Cancel any interactive modes underway
    pub fn cancel_interactive(&mut self) {
        self.preparing_reply = None;
    }

    /// Construct an Element that displays the channel view
    pub fn view<'a>(
        &'a self,
        nodes: &'a HashMap<u32, NodeInfo>,
        enable_position: bool,
        enable_my_info: bool,
        device_view: &'a DeviceView,
        config: &'a Config,
    ) -> Element<'a, Message> {
        let channel_view_content = self.channel_view(nodes, enable_position, enable_my_info);

        if device_view.forwarding_message.is_some() {
            self.channel_picker(channel_view_content, device_view, config)
        } else {
            channel_view_content
        }
    }

    fn channel_view<'a>(
        &'a self,
        nodes: &'a HashMap<u32, NodeInfo>,
        enable_position: bool,
        enable_my_info: bool,
    ) -> Element<'a, Message> {
        let mut channel_view_content = Column::new().padding(right(10));

        let message_area: Element<'a, Message> = if self.entries.is_empty() {
            Self::empty_view()
        } else {
            let mut previous_day = u32::MIN;

            // Add a view to the column for each of the entries in this Channel
            for entry in self.entries.values() {
                let message_day = entry.time().day();

                // Add a day separator when the day of an entry changes
                if message_day != previous_day {
                    channel_view_content =
                        channel_view_content.push(Self::day_separator(&entry.time()));
                    previous_day = message_day;
                }

                channel_view_content = channel_view_content.push(entry.view(
                    &self.entries,
                    nodes,
                    &self.channel_id,
                    entry.source_node(self.source),
                ));
            }

            // Wrap the list of messages in a scrollable container, with a scrollbar
            scrollable(channel_view_content)
                .direction({
                    let scrollbar = Scrollbar::new().width(10.0);
                    scrollable::Direction::Vertical(scrollbar)
                })
                .style(scrollbar_style)
                .width(Fill)
                .height(Fill)
                .into()
        };

        // A row of action buttons at the bottom of the channel view - this could be made
        // a menu or something different in the future
        let mut send_position_button = button(text("Send Position 📌")).style(button_chip_style);
        if enable_position {
            send_position_button = send_position_button.on_press(DeviceViewEvent(
                SendPositionMessage(self.channel_id.clone()),
            ));
        }

        let mut send_info_button = button(text("Send Info ⓘ")).style(button_chip_style);
        if enable_my_info {
            send_info_button = send_info_button
                .on_press(DeviceViewEvent(SendInfoMessage(self.channel_id.clone())));
        }

        let channel_buttons = Row::new()
            .padding([2, 0])
            .push(send_position_button)
            .push(Space::new().width(6))
            .push(send_info_button);

        // Place the scrollable in a column, with a row of buttons at the bottom
        let mut column = Column::new()
            .padding(4)
            .push(message_area)
            .push(channel_buttons);

        // If we are replying to a message, add a row at the bottom of the channel view with the original text
        if let Some(entry_id) = &self.preparing_reply {
            column = self.replying_to(column, entry_id);
        }

        // Add the input box at the bottom of the channel view
        column.push(self.input_box()).into()
    }

    fn empty_view<'a>() -> Element<'a, Message> {
        Container::new(Column::new().push(text("No messages sent or received yet.").align_x(Center).size(20))
                           .push(text("You can use the text box at the bottom of the screen to send a text message, or the buttons to send your position or node info").align_x(Center).size(20)).align_x(Center))
            .padding(10)
            .width(Fill)
            .align_y(Center)
            .height(Fill)
            .align_x(Center)
            .into()
    }

    /// Create a modal dialog with a channel list view inside it that the user can use to
    /// select a channel/node from
    fn channel_picker<'a>(
        &'a self,
        content: Element<'a, Message>,
        device_view: &'a DeviceView,
        config: &'a Config,
    ) -> Element<'a, Message> {
        let select = |channel_number: ChannelId| DeviceViewEvent(ForwardMessage(channel_number));
        let inner_picker = Column::new()
            .push(
                container(
                    text("Chose channel or node to forward message to")
                        .size(18)
                        .width(Fill)
                        .font(Font {
                            weight: Weight::Bold,
                            ..Default::default()
                        })
                        .align_x(Center),
                )
                .style(picker_header_style)
                .padding(4),
            )
            .push(device_view.channel_and_node_list(config, false, select));
        let picker = container(inner_picker)
            .style(tooltip_style)
            .width(400)
            .height(600);
        Self::modal(content, picker, DeviceViewEvent(StopForwardingMessage))
    }

    /// Function to create a modal dialog in the middle of the screen
    fn modal<'a, Message>(
        base: impl Into<Element<'a, Message>>,
        content: impl Into<Element<'a, Message>>,
        on_blur: Message,
    ) -> Element<'a, Message>
    where
        Message: Clone + 'a,
    {
        stack![
            base.into(),
            opaque(
                mouse_area(center(opaque(content)).style(|_theme| {
                    container::Style {
                        background: Some(
                            Color {
                                a: 0.8,
                                ..Color::BLACK
                            }
                            .into(),
                        ),
                        ..container::Style::default()
                    }
                }))
                .on_press(on_blur)
            )
        ]
        .into()
    }

    /// Add a row that explains we are replying to a prior message
    fn replying_to<'a>(
        &'a self,
        column: Column<'a, Message>,
        entry_id: &u32,
    ) -> Column<'a, Message> {
        if let Some(original_text) = ChannelViewEntry::reply_quote(&self.entries, entry_id) {
            let cancel_reply_button: Button<Message> = button(text("⨂").size(16))
                .on_press(DeviceViewEvent(ChannelMsg(CancelPrepareReply)))
                .style(button_chip_style)
                .padding(0);

            column.push(
                container(
                    Row::new()
                        .align_y(Center)
                        .padding(2)
                        .push(Space::new().width(24))
                        .push(text(original_text).font(Font {
                            style: Italic,
                            ..Default::default()
                        }))
                        .push(Space::new().width(8))
                        .push(cancel_reply_button),
                )
                .width(Fill)
                .style(reply_to_style),
            )
        } else {
            column
        }
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
        let mut clear_button = button(text("⨂").size(18))
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
            .push(Space::new().width(4.0))
            .push(clear_button)
            .push(Space::new().width(4.0))
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
        let oldest_message = ChannelViewEntry::new(NewTextMessage("Hello 1".to_string()), 1, 1);
        tokio::time::sleep(Duration::from_millis(1500)).await;

        let middle_message = ChannelViewEntry::new(NewTextMessage("Hello 2".to_string()), 2, 1000);
        tokio::time::sleep(Duration::from_millis(1500)).await;

        let newest_message = ChannelViewEntry::new(NewTextMessage("Hello 3".to_string()), 1, 500);

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
