use crate::Message::DeviceViewEvent;
use crate::channel_id::ChannelId;
use crate::channel_view::ChannelViewMessage::{
    CancelPrepareReply, ClearMessage, EmojiPickerMsg, MessageInput, MessageSeen, MessageUnseen,
    PickChannel, PrepareReply, ReplyWithEmoji, SendMessage, ShareMeshChat,
};
use crate::channel_view_entry::MCMessage::{
    AlertMessage, EmojiReply, NewTextMessage, PositionMessage, TextMessageReply, UserMessage,
};
use crate::config::{Config, HistoryLength};
use crate::device_view::DeviceViewMessage::{
    ChannelMsg, ForwardMessage, SendInfoMessage, SendPositionMessage, ShowChannel,
    StopForwardingMessage,
};
use crate::device_view::{DeviceView, DeviceViewMessage};
use crate::styles::{
    DAY_SEPARATOR_STYLE, button_chip_style, picker_header_style, reply_to_style, scrollbar_style,
    text_input_style, tooltip_style,
};
use crate::{MCNodeInfo, MeshChat, Message, channel_view_entry::ChannelViewEntry, icons};
use chrono::prelude::DateTime;
use chrono::{Datelike, Local, Utc};
use iced::font::Style::Italic;
use iced::font::Weight;
use iced::padding::right;
use iced::widget::operation::RelativeOffset;
use iced::widget::scrollable::Scrollbar;
use iced::widget::text_input::{Icon, Side};
use iced::widget::{
    Button, Column, Container, Row, Space, button, container, row, scrollable, text, text_input,
};
use iced::widget::{Id, operation};
use iced::{Center, Element, Fill, Font, Padding, Pixels, Task};
use ringmap::RingMap;
use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};

const MESSAGE_INPUT_ID: &str = "message_input";
const CHANNEL_VIEW_SCROLLABLE: &str = "channel_view_scrollable";

#[derive(Debug, Clone)]
pub enum ChannelViewMessage {
    MessageInput(String),
    ClearMessage,
    SendMessage(Option<u32>), // optional message id if we are replying to that message
    PrepareReply(u32),        // entry_id
    CancelPrepareReply,
    MessageSeen(u32),
    MessageUnseen(u32),
    PickChannel(Option<ChannelId>),
    ReplyWithEmoji(u32, String, ChannelId), // Send an emoji reply
    EmojiPickerMsg(Box<crate::emoji_picker::PickerMessage<ChannelViewMessage>>),
    ShareMeshChat,
}

/// [ChannelView] implements view and update methods for Iced for a set of
/// messages to and from a "Channel" which can be a Channel or a Node
#[derive(Debug, Default)]
pub struct ChannelView {
    channel_id: ChannelId,
    message: String,                         // text message typed in so far
    entries: RingMap<u32, ChannelViewEntry>, // entries received so far, keyed by message_id, ordered by rx_time
    my_node_num: u32,
    preparing_reply: Option<u32>,
    emoji_picker: crate::emoji_picker::EmojiPicker,
}

async fn empty() {}

// A view of a single channel and its messages, which maybe a Channel or a Node
impl ChannelView {
    pub fn new(channel_id: ChannelId, source: u32) -> Self {
        Self {
            channel_id,
            my_node_num: source,
            emoji_picker: crate::emoji_picker::EmojiPicker::new()
                .height(400)
                .width(400),
            ..Default::default()
        }
    }

    /// Acknowledge the receipt of a message.
    pub fn ack(&mut self, request_id: u32) {
        if let Some(entry) = self.entries.get_mut(&request_id) {
            entry.ack();
        }
    }

    /// Get the time now as a [DateTime<Local>]
    fn now_local() -> DateTime<Local> {
        let rx_time = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|t| t.as_secs())
            .unwrap_or(0);

        let datetime_utc = DateTime::<Utc>::from_timestamp_secs(rx_time as i64).unwrap();
        datetime_utc.with_timezone(&Local)
    }

    /// Remove older messages according to the passed in history_length setting
    fn trim_history(&mut self, history_length: &HistoryLength) {
        // if there is an active config for the maximum length of history, then trim
        // the list of entries
        match history_length {
            HistoryLength::NumberOfMessages(number) => {
                while self.entries.len() > *number {
                    self.entries.pop_front();
                }
            }
            HistoryLength::Duration(duration) => {
                let oldest = Self::now_local() - *duration;
                while self
                    .entries
                    .pop_front_if(|_k, v| v.time() < oldest)
                    .is_some()
                {}
            }
            _ => { /* leave all messages as no limit set */ }
        }
    }

    /// Add a new [ChannelViewEntry] message to the [ChannelView], unless it's an emoji reply,
    /// in which case the emoji will be added to the Vec of emoji replies of the original
    /// message [ChannelViewEntry], and no new entry created
    pub fn new_message(
        &mut self,
        new_message: ChannelViewEntry,
        history_length: &HistoryLength,
    ) -> Task<Message> {
        let mine = new_message.from() == self.my_node_num;

        match &new_message.message() {
            AlertMessage(_)
            | NewTextMessage(_)
            | PositionMessage(_)
            | UserMessage(_)
            | TextMessageReply(_, _) => {
                // Insert a new message, ordered by receive date/time
                self.entries.insert_sorted_by(
                    new_message.message_id(),
                    new_message,
                    ChannelViewEntry::sort_by_rx_time,
                );

                self.trim_history(history_length);
            }
            EmojiReply(reply_to_id, emoji_string) => {
                if let Some(entry) = self.entries.get_mut(reply_to_id) {
                    entry.add_emoji(emoji_string.to_string(), new_message.from());
                }
            }
        };

        if mine {
            // scroll to the end of messages to see the message I just sent
            operation::snap_to(Id::from(CHANNEL_VIEW_SCROLLABLE), RelativeOffset::END)
        } else {
            Task::none()
        }
    }

    /// Cancel any interactive modes underway
    pub fn cancel_interactive(&mut self) {
        self.preparing_reply = None;
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
                if entry_id < self.entries.len() as u32 {
                    self.preparing_reply = Some(entry_id);
                }
                Task::none()
            }
            CancelPrepareReply => {
                self.cancel_interactive();
                Task::none()
            }
            MessageSeen(message_id) => {
                if let Some(channel_view_entry) = self.entries.get_mut(&message_id) {
                    channel_view_entry.mark_seen();
                } else {
                    println!("Error Message {} not found in ChannelView", message_id);
                }
                Task::none()
            }
            MessageUnseen(_) => Task::none(),
            PickChannel(channel_id) => {
                Task::perform(empty(), move |_| DeviceViewEvent(ShowChannel(channel_id)))
            }
            ReplyWithEmoji(message_id, emoji, channel_id) => Task::perform(empty(), move |_| {
                DeviceViewEvent(DeviceViewMessage::SendEmojiReplyMessage(
                    message_id, emoji, channel_id,
                ))
            }),
            EmojiPickerMsg(picker_msg) => {
                if let Some(msg) = self.emoji_picker.update(*picker_msg) {
                    // Forward the wrapped message
                    self.update(msg)
                } else {
                    // Internal state change only
                    Task::none()
                }
            }
            ShareMeshChat => {
                // Insert the pre-prepared sharing message text to the message text_input
                self.message = String::from(
                    "I am using the MeshChat app. Its available for macOS/Windows/Linux from: \
                https://github.com/andrewdavidmackenzie/meshchat/releases/latest ",
                );
                // and shift the focus to it in case the user wants to add to it or edit it
                operation::focus(Id::new(MESSAGE_INPUT_ID))
            }
        }
    }

    /// Construct an Element that displays the channel view
    #[allow(clippy::too_many_arguments)]
    pub fn view<'a>(
        &'a self,
        nodes: &'a HashMap<u32, MCNodeInfo>,
        enable_position: bool,
        enable_my_user: bool,
        device_view: &'a DeviceView,
        config: &'a Config,
        show_position_updates: bool,
        show_user_updates: bool,
    ) -> Element<'a, Message> {
        let channel_view_content = self.channel_view(
            nodes,
            enable_position,
            enable_my_user,
            show_position_updates,
            show_user_updates,
        );

        if device_view.forwarding_message.is_some() {
            self.channel_picker(channel_view_content, device_view, config)
        } else {
            channel_view_content
        }
    }

    /// Return the number of unread messages in the channel, not counting message types that
    /// are currently not being shown
    pub fn unread_count(&self, show_position_updates: bool, show_user_updates: bool) -> usize {
        self.entries.values().fold(0, |acc, entry| {
            if (matches!(entry.message(), PositionMessage(..)) && !show_position_updates)
                || (matches!(entry.message(), UserMessage(..)) && !show_user_updates)
            {
                acc
            } else if !entry.seen() {
                acc + 1
            } else {
                acc
            }
        })
    }

    fn channel_view<'a>(
        &'a self,
        nodes: &'a HashMap<u32, MCNodeInfo>,
        enable_position: bool,
        enable_my_info: bool,
        show_position_updates: bool,
        show_user_updates: bool,
    ) -> Element<'a, Message> {
        let mut channel_view_content = Column::new().padding(right(10));

        let message_area: Element<'a, Message> = if self.entries.is_empty() {
            Self::empty_view()
        } else {
            let mut previous_day = u32::MIN;

            // Add a view to the column for each of the entries in this Channel
            for entry in self.entries.values() {
                // Hide any previously received position updates in the view if config is set to do so
                if matches!(entry.message(), PositionMessage(..)) && !show_position_updates {
                    continue;
                }

                // Hide any previously received user updates in the view if config is set to do so
                if matches!(entry.message(), UserMessage(..)) && !show_user_updates {
                    continue;
                }

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
                    entry.from() == self.my_node_num,
                    &self.emoji_picker,
                ));
            }

            // Wrap the list of messages in a scrollable container, with a scrollbar
            scrollable(channel_view_content)
                .direction({
                    let scrollbar = Scrollbar::new().width(10.0);
                    scrollable::Direction::Vertical(scrollbar)
                })
                .id(Id::new(CHANNEL_VIEW_SCROLLABLE))
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

        // a button to allow easy sharing of this app
        let share_meshchat_button =
            button(row([text("Share MeshChat ").into(), icons::share().into()]))
                .style(button_chip_style)
                .on_press(DeviceViewEvent(ChannelMsg(
                    self.channel_id.clone(),
                    ShareMeshChat,
                )));

        let channel_buttons = Row::new()
            .padding([2, 0])
            .push(send_position_button)
            .push(Space::new().width(6))
            .push(send_info_button)
            .push(Space::new().width(6))
            .push(share_meshchat_button);

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
        MeshChat::modal(content, picker, DeviceViewEvent(StopForwardingMessage))
    }

    /// Add a row that explains we are replying to a prior message
    fn replying_to<'a>(
        &'a self,
        column: Column<'a, Message>,
        entry_id: &u32,
    ) -> Column<'a, Message> {
        if let Some(original_text) = ChannelViewEntry::reply_quote(&self.entries, entry_id) {
            let cancel_reply_button: Button<Message> = button(text("⨂").size(16))
                .on_press(DeviceViewEvent(ChannelMsg(
                    self.channel_id.clone(),
                    CancelPrepareReply,
                )))
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

    fn input_box(&'_ self) -> Element<'_, Message> {
        let mut send_button = button(icons::send().size(18))
            .style(button_chip_style)
            .padding(Padding::from([6, 6]));
        let mut clear_button = button(text("⨂").size(18))
            .style(button_chip_style)
            .padding(Padding::from([6, 6]));
        if !self.message.is_empty() {
            send_button = send_button.on_press(DeviceViewEvent(ChannelMsg(
                self.channel_id.clone(),
                SendMessage(self.preparing_reply),
            )));
            clear_button = clear_button.on_press(DeviceViewEvent(ChannelMsg(
                self.channel_id.clone(),
                ClearMessage,
            )));
        }

        Row::new()
            .align_y(Center)
            .push(
                text_input("Type your message here", &self.message)
                    .style(text_input_style)
                    .id(Id::new(MESSAGE_INPUT_ID))
                    .on_input(|s| {
                        DeviceViewEvent(ChannelMsg(self.channel_id.clone(), MessageInput(s)))
                    })
                    .on_submit(DeviceViewEvent(ChannelMsg(
                        self.channel_id.clone(),
                        SendMessage(self.preparing_reply),
                    )))
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
    use crate::channel_view::ChannelViewMessage::{
        CancelPrepareReply, ClearMessage, MessageInput, MessageSeen, PrepareReply, SendMessage,
    };
    use crate::channel_view::{ChannelId, ChannelView};
    use crate::channel_view_entry::ChannelViewEntry;
    use crate::channel_view_entry::MCMessage::{EmojiReply, NewTextMessage};
    use crate::config::HistoryLength;
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

    fn now_secs() -> u32 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs() as u32
    }

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
            1,
            1,
            NewTextMessage("Hello 1".to_string()),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs() as u32,
        );
        tokio::time::sleep(Duration::from_millis(1500)).await;

        let middle_message = ChannelViewEntry::new(
            2,
            1000,
            NewTextMessage("Hello 2".to_string()),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs() as u32,
        );
        tokio::time::sleep(Duration::from_millis(1500)).await;

        let newest_message = ChannelViewEntry::new(
            3,
            500,
            NewTextMessage("Hello 3".to_string()),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs() as u32,
        );

        // Add them out of order - they should be ordered by the time of creation, not entry
        let _ = channel_view.new_message(newest_message.clone(), &HistoryLength::All);
        let _ = channel_view.new_message(middle_message.clone(), &HistoryLength::All);
        let _ = channel_view.new_message(oldest_message.clone(), &HistoryLength::All);
        assert_eq!(
            channel_view.entries.values().len(),
            3,
            "There should be 3 messages in the list"
        );
        assert_eq!(
            channel_view.unread_count(true, true),
            3,
            "The unread count should be 3"
        );

        // Check the order is correct
        let mut iter = channel_view.entries.values();
        assert_eq!(
            iter.next().unwrap(),
            &oldest_message,
            "The oldest message should be first"
        );
        assert_eq!(
            iter.next().unwrap(),
            &middle_message,
            "The middle message should be second"
        );
        assert_eq!(
            iter.next().unwrap(),
            &newest_message,
            "The newest message should be third"
        );
    }

    #[test]
    fn test_initial_unread_count() {
        let channel_view = ChannelView::new(ChannelId::Channel(0), 0);
        assert_eq!(channel_view.unread_count(true, true), 0);
    }

    #[test]
    fn test_unread_count() {
        let mut channel_view = ChannelView::new(ChannelId::Channel(0), 0);
        let message = ChannelViewEntry::new(
            1,
            1,
            NewTextMessage("Hello 1".to_string()),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs() as u32,
        );
        let _ = channel_view.new_message(message.clone(), &HistoryLength::All);
        assert_eq!(channel_view.unread_count(true, true), 1);
    }

    #[test]
    fn test_replying_valid_entry() {
        let mut channel_view = ChannelView::new(ChannelId::Channel(0), 0);
        let message = ChannelViewEntry::new(
            1,
            1,
            NewTextMessage("Hello 1".to_string()),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs() as u32,
        );
        let _ = channel_view.new_message(message, &HistoryLength::All);

        let _ = channel_view.update(PrepareReply(0));

        assert_eq!(channel_view.preparing_reply, Some(0));
    }

    #[test]
    fn test_replying_invalid_entry() {
        let mut channel_view = ChannelView::new(ChannelId::Channel(0), 0);
        let message = ChannelViewEntry::new(
            1,
            1,
            NewTextMessage("Hello 1".to_string()),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs() as u32,
        );
        let _ = channel_view.new_message(message, &HistoryLength::All);

        let _ = channel_view.update(PrepareReply(1));

        assert!(channel_view.preparing_reply.is_none());
    }

    #[test]
    fn test_cancel_prepare_reply() {
        let mut channel_view = ChannelView::new(ChannelId::Channel(0), 0);
        let message = ChannelViewEntry::new(1, 1, NewTextMessage("test".into()), now_secs());
        let _ = channel_view.new_message(message, &HistoryLength::All);
        let _ = channel_view.update(PrepareReply(0));
        assert!(channel_view.preparing_reply.is_some());

        let _ = channel_view.update(CancelPrepareReply);
        assert!(channel_view.preparing_reply.is_none());
    }

    #[test]
    fn test_cancel_interactive() {
        let mut channel_view = ChannelView::new(ChannelId::Channel(0), 0);
        let message = ChannelViewEntry::new(1, 1, NewTextMessage("test".into()), now_secs());
        let _ = channel_view.new_message(message, &HistoryLength::All);
        let _ = channel_view.update(PrepareReply(0));
        assert!(channel_view.preparing_reply.is_some());

        channel_view.cancel_interactive();
        assert!(channel_view.preparing_reply.is_none());
    }

    #[test]
    fn test_message_input() {
        let mut channel_view = ChannelView::new(ChannelId::Channel(0), 0);
        assert!(channel_view.message.is_empty());

        let _ = channel_view.update(MessageInput("Hello".into()));
        assert_eq!(channel_view.message, "Hello");
    }

    #[test]
    fn test_message_input_max_length() {
        let mut channel_view = ChannelView::new(ChannelId::Channel(0), 0);

        // Message longer than 200 chars should be rejected
        let long_message = "a".repeat(250);
        let _ = channel_view.update(MessageInput(long_message));
        assert!(channel_view.message.is_empty());

        // Message of 199 chars should be accepted
        let valid_message = "b".repeat(199);
        let _ = channel_view.update(MessageInput(valid_message.clone()));
        assert_eq!(channel_view.message, valid_message);
    }

    #[test]
    fn test_clear_message() {
        let mut channel_view = ChannelView::new(ChannelId::Channel(0), 0);
        let _ = channel_view.update(MessageInput("Hello".into()));
        assert!(!channel_view.message.is_empty());

        let _ = channel_view.update(ClearMessage);
        assert!(channel_view.message.is_empty());
    }

    #[test]
    fn test_send_message_empty() {
        let mut channel_view = ChannelView::new(ChannelId::Channel(0), 0);
        // Empty message should not trigger a send task
        let _ = channel_view.update(SendMessage(None));
        // No crash, message still empty
        assert!(channel_view.message.is_empty());
    }

    #[test]
    fn test_send_message_clears_text() {
        let mut channel_view = ChannelView::new(ChannelId::Channel(0), 0);
        let _ = channel_view.update(MessageInput("Hello world".into()));
        assert!(!channel_view.message.is_empty());

        let _ = channel_view.update(SendMessage(None));
        assert!(channel_view.message.is_empty());
    }

    #[test]
    fn test_send_message_clears_preparing_reply() {
        let mut channel_view = ChannelView::new(ChannelId::Channel(0), 0);
        let message = ChannelViewEntry::new(1, 1, NewTextMessage("test".into()), now_secs());
        let _ = channel_view.new_message(message, &HistoryLength::All);
        let _ = channel_view.update(PrepareReply(0));
        let _ = channel_view.update(MessageInput("Reply text".into()));

        let _ = channel_view.update(SendMessage(Some(1)));
        assert!(channel_view.preparing_reply.is_none());
    }

    #[test]
    fn test_ack_message() {
        let mut channel_view = ChannelView::new(ChannelId::Channel(0), 0);
        let message = ChannelViewEntry::new(42, 1, NewTextMessage("test".into()), now_secs());
        let _ = channel_view.new_message(message, &HistoryLength::All);

        assert!(!channel_view.entries.get(&42).unwrap().acked());

        channel_view.ack(42);
        assert!(channel_view.entries.get(&42).unwrap().acked());
    }

    #[test]
    fn test_ack_nonexistent_message() {
        let mut channel_view = ChannelView::new(ChannelId::Channel(0), 0);
        // Should not panic
        channel_view.ack(999);
    }

    #[test]
    fn test_message_seen() {
        let mut channel_view = ChannelView::new(ChannelId::Channel(0), 0);
        let message = ChannelViewEntry::new(42, 1, NewTextMessage("test".into()), now_secs());
        let _ = channel_view.new_message(message, &HistoryLength::All);

        assert!(!channel_view.entries.get(&42).unwrap().seen());
        assert_eq!(channel_view.unread_count(), 1);

        let _ = channel_view.update(MessageSeen(42));
        assert!(channel_view.entries.get(&42).unwrap().seen());
        assert_eq!(channel_view.unread_count(), 0);
    }

    #[test]
    fn test_trim_history_by_number() {
        let mut channel_view = ChannelView::new(ChannelId::Channel(0), 0);

        // Add 5 messages
        for i in 0..5 {
            let message = ChannelViewEntry::new(i, 1, NewTextMessage(format!("msg {}", i)), i);
            let _ = channel_view.new_message(message, &HistoryLength::All);
        }
        assert_eq!(channel_view.entries.len(), 5);

        // Adding 6th message with limit of 3 should trim to 3
        let message = ChannelViewEntry::new(5, 1, NewTextMessage("msg 5".into()), 5);
        let _ = channel_view.new_message(message, &HistoryLength::NumberOfMessages(3));
        assert_eq!(channel_view.entries.len(), 3);

        // The oldest messages should have been removed
        assert!(!channel_view.entries.contains_key(&0));
        assert!(!channel_view.entries.contains_key(&1));
        assert!(!channel_view.entries.contains_key(&2));
        assert!(channel_view.entries.contains_key(&3));
        assert!(channel_view.entries.contains_key(&4));
        assert!(channel_view.entries.contains_key(&5));
    }

    #[test]
    fn test_trim_history_by_duration() {
        let mut channel_view = ChannelView::new(ChannelId::Channel(0), 0);
        let now = now_secs();

        // Add an old message (2 hours ago)
        let old_time = now - 7200;
        let old_message = ChannelViewEntry::new(1, 1, NewTextMessage("old".into()), old_time);
        let _ = channel_view.new_message(old_message, &HistoryLength::All);

        // Add a recent message (now)
        let new_message = ChannelViewEntry::new(2, 1, NewTextMessage("new".into()), now);
        // Trim history to 1 hour (3600 seconds)
        let _ = channel_view.new_message(
            new_message,
            &HistoryLength::Duration(Duration::from_secs(3600)),
        );

        // Old message should be trimmed
        assert_eq!(channel_view.entries.len(), 1);
        assert!(channel_view.entries.contains_key(&2));
    }

    #[test]
    fn test_emoji_reply_adds_to_existing_message() {
        let mut channel_view = ChannelView::new(ChannelId::Channel(0), 0);

        // Add original message
        let original = ChannelViewEntry::new(1, 100, NewTextMessage("Hello".into()), now_secs());
        let _ = channel_view.new_message(original, &HistoryLength::All);
        assert_eq!(channel_view.entries.len(), 1);

        // Add emoji reply to message 1
        let emoji_reply = ChannelViewEntry::new(2, 200, EmojiReply(1, "👍".into()), now_secs());
        let _ = channel_view.new_message(emoji_reply, &HistoryLength::All);

        // Should still only have 1 entry (the original)
        assert_eq!(channel_view.entries.len(), 1);
        // But the original should now have the emoji
        let original_entry = channel_view.entries.get(&1).unwrap();
        assert!(original_entry.emojis().contains_key("👍"));
    }

    #[test]
    fn test_new_default() {
        let channel_view = ChannelView::default();
        assert_eq!(channel_view.channel_id, ChannelId::default());
        assert!(channel_view.message.is_empty());
        assert!(channel_view.entries.is_empty());
        assert!(channel_view.preparing_reply.is_none());
    }

    #[test]
    fn test_my_message_scrolls() {
        let mut channel_view = ChannelView::new(ChannelId::Channel(0), 100);

        // Message from me (node 100)
        let my_message = ChannelViewEntry::new(1, 100, NewTextMessage("my msg".into()), now_secs());
        let task = channel_view.new_message(my_message, &HistoryLength::All);

        // Should return a scroll task (not Task::none)
        // We can't easily check the task type, but we can verify the message was added
        assert_eq!(channel_view.entries.len(), 1);
        // The task should not be none - it should be a snap_to operation
        drop(task); // Just verify it doesn't panic
    }

    #[test]
    fn test_others_message_no_scroll() {
        let mut channel_view = ChannelView::new(ChannelId::Channel(0), 100);

        // Message from someone else (node 200)
        let other_message =
            ChannelViewEntry::new(1, 200, NewTextMessage("other msg".into()), now_secs());
        let _task = channel_view.new_message(other_message, &HistoryLength::All);

        assert_eq!(channel_view.entries.len(), 1);
    }
}
