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
    text_input_button_style, text_input_container_style, text_input_style, tooltip_style,
};
use crate::widgets::emoji_picker::{EmojiPicker, PickerMessage};
use crate::{MCNodeInfo, MeshChat, Message, channel_view_entry::ChannelViewEntry, icons};
use chrono::prelude::DateTime;
use chrono::{Datelike, Local, Utc};
use iced::font::Style::Italic;
use iced::font::Weight;
use iced::padding::right;
use iced::widget::operation::RelativeOffset;
use iced::widget::scrollable::Scrollbar;
use iced::widget::{
    Button, Column, Container, Row, Space, button, container, row, scrollable, text, text_input,
};
use iced::widget::{Id, operation};
use iced::{Center, Element, Fill, Font, Padding, Task};
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
    EmojiPickerMsg(Box<PickerMessage<ChannelViewMessage>>),
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
    emoji_picker: EmojiPicker,
}

async fn empty() {}

// A view of a single channel and its messages, which maybe a Channel or a Node
impl ChannelView {
    pub fn new(channel_id: ChannelId, my_node_num: u32) -> Self {
        Self {
            channel_id,
            my_node_num,
            emoji_picker: EmojiPicker::new().height(400).width(400),
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

        let datetime_utc = DateTime::<Utc>::from_timestamp_secs(rx_time as i64).unwrap_or_default();
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
                    let channel_id = self.channel_id;
                    self.preparing_reply = None;
                    Task::perform(empty(), move |_| {
                        DeviceViewEvent(DeviceViewMessage::SendTextMessage(
                            msg,
                            channel_id,
                            reply_to_id,
                        ))
                    })
                } else {
                    Task::none()
                }
            }
            PrepareReply(entry_id) => {
                if self.entries.contains_key(&entry_id) {
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
            send_position_button = send_position_button
                .on_press(DeviceViewEvent(SendPositionMessage(self.channel_id)));
        }

        let mut send_info_button = button(text("Send Info ⓘ")).style(button_chip_style);
        if enable_my_info {
            send_info_button =
                send_info_button.on_press(DeviceViewEvent(SendInfoMessage(self.channel_id)));
        }

        // a button to allow easy sharing of this app
        let share_meshchat_button =
            button(row([text("Share MeshChat ").into(), icons::share().into()]))
                .style(button_chip_style)
                .on_press(DeviceViewEvent(ChannelMsg(self.channel_id, ShareMeshChat)));

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
                    self.channel_id,
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

    fn send_button(&'_ self) -> Button<'_, Message> {
        let mut send_button = button(icons::send().size(18))
            .style(text_input_button_style)
            .padding(Padding::from([6, 6]));

        if !self.message.is_empty() {
            send_button = send_button.on_press(DeviceViewEvent(ChannelMsg(
                self.channel_id,
                SendMessage(self.preparing_reply),
            )));
        }

        send_button
    }

    fn clear_button(&'_ self) -> Button<'_, Message> {
        let mut clear_button = button(text("⨂").size(18))
            .style(text_input_button_style)
            .padding(Padding::from([6, 6]));
        if !self.message.is_empty() {
            clear_button =
                clear_button.on_press(DeviceViewEvent(ChannelMsg(self.channel_id, ClearMessage)));
        }

        clear_button
    }

    fn input_box(&'_ self) -> Element<'_, Message> {
        container(
            container(
                Row::new()
                    .push(Space::new().width(9.0))
                    .push(
                        text_input("Type your message here", &self.message)
                            .style(text_input_style)
                            .padding([4, 4])
                            .id(Id::new(MESSAGE_INPUT_ID))
                            .on_input(|s| {
                                DeviceViewEvent(ChannelMsg(self.channel_id, MessageInput(s)))
                            })
                            .on_submit(DeviceViewEvent(ChannelMsg(
                                self.channel_id,
                                SendMessage(self.preparing_reply),
                            ))),
                    )
                    .push(Space::new().width(4.0))
                    .push(self.clear_button())
                    .push(self.send_button())
                    .push(Space::new().width(4.0))
                    .align_y(Center),
            )
            .style(text_input_container_style),
        )
        .padding(Padding::from([8, 8]))
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
            .expect("Could not get time")
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
                .expect("system time should be after UNIX_EPOCH")
                .as_secs() as u32,
        );
        tokio::time::sleep(Duration::from_millis(1500)).await;

        let middle_message = ChannelViewEntry::new(
            2,
            1000,
            NewTextMessage("Hello 2".to_string()),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system time should be after UNIX_EPOCH")
                .as_secs() as u32,
        );
        tokio::time::sleep(Duration::from_millis(1500)).await;

        let newest_message = ChannelViewEntry::new(
            3,
            500,
            NewTextMessage("Hello 3".to_string()),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system time should be after UNIX_EPOCH")
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
            iter.next().expect("iterator should have first entry"),
            &oldest_message,
            "The oldest message should be first"
        );
        assert_eq!(
            iter.next().expect("iterator should have second entry"),
            &middle_message,
            "The middle message should be second"
        );
        assert_eq!(
            iter.next().expect("iterator should have third entry"),
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
                .expect("system time should be after UNIX_EPOCH")
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
                .expect("system time should be after UNIX_EPOCH")
                .as_secs() as u32,
        );
        let _ = channel_view.new_message(message, &HistoryLength::All);

        // Use the actual message ID (1), not an index
        let _ = channel_view.update(PrepareReply(1));

        assert_eq!(channel_view.preparing_reply, Some(1));
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
                .expect("system time should be after UNIX_EPOCH")
                .as_secs() as u32,
        );
        let _ = channel_view.new_message(message, &HistoryLength::All);

        // Use a non-existent message ID (999) - should not set preparing_reply
        let _ = channel_view.update(PrepareReply(999));

        assert!(channel_view.preparing_reply.is_none());
    }

    #[test]
    fn test_cancel_prepare_reply() {
        let mut channel_view = ChannelView::new(ChannelId::Channel(0), 0);
        let message = ChannelViewEntry::new(1, 1, NewTextMessage("test".into()), now_secs());
        let _ = channel_view.new_message(message, &HistoryLength::All);
        let _ = channel_view.update(PrepareReply(1)); // Use actual message ID
        assert!(channel_view.preparing_reply.is_some());

        let _ = channel_view.update(CancelPrepareReply);
        assert!(channel_view.preparing_reply.is_none());
    }

    #[test]
    fn test_cancel_interactive() {
        let mut channel_view = ChannelView::new(ChannelId::Channel(0), 0);
        let message = ChannelViewEntry::new(1, 1, NewTextMessage("test".into()), now_secs());
        let _ = channel_view.new_message(message, &HistoryLength::All);
        let _ = channel_view.update(PrepareReply(1)); // Use actual message ID
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
        let _ = channel_view.update(PrepareReply(1)); // Use actual message ID
        let _ = channel_view.update(MessageInput("Reply text".into()));

        let _ = channel_view.update(SendMessage(Some(1)));
        assert!(channel_view.preparing_reply.is_none());
    }

    #[test]
    fn test_ack_message() {
        let mut channel_view = ChannelView::new(ChannelId::Channel(0), 0);
        let message = ChannelViewEntry::new(42, 1, NewTextMessage("test".into()), now_secs());
        let _ = channel_view.new_message(message, &HistoryLength::All);

        assert!(
            !channel_view
                .entries
                .get(&42)
                .expect("entry 42 should exist")
                .acked()
        );

        channel_view.ack(42);
        assert!(
            channel_view
                .entries
                .get(&42)
                .expect("entry 42 should exist after ack")
                .acked()
        );
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

        assert!(
            !channel_view
                .entries
                .get(&42)
                .expect("entry 42 should exist")
                .seen()
        );
        assert_eq!(channel_view.unread_count(true, true), 1);

        let _ = channel_view.update(MessageSeen(42));
        assert!(
            channel_view
                .entries
                .get(&42)
                .expect("entry 42 should exist after marking seen")
                .seen()
        );
        assert_eq!(channel_view.unread_count(true, true), 0);
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

        // Adding the 6th message with a limit of 3 should trim to 3
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

        // Add the original message
        let original = ChannelViewEntry::new(1, 100, NewTextMessage("Hello".into()), now_secs());
        let _ = channel_view.new_message(original, &HistoryLength::All);
        assert_eq!(channel_view.entries.len(), 1);

        // Add emoji reply to message 1
        let emoji_reply = ChannelViewEntry::new(2, 200, EmojiReply(1, "👍".into()), now_secs());
        let _ = channel_view.new_message(emoji_reply, &HistoryLength::All);

        // Should still only have 1 entry (the original)
        assert_eq!(channel_view.entries.len(), 1);
        // But the original should now have the emoji
        let original_entry = channel_view
            .entries
            .get(&1)
            .expect("original entry should exist after emoji reply");
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

    #[test]
    fn test_channel_view_debug() {
        let channel_view = ChannelView::new(ChannelId::Channel(0), 100);
        let debug_str = format!("{:?}", channel_view);
        assert!(debug_str.contains("ChannelView"));
    }

    #[test]
    fn test_channel_view_message_debug() {
        let msg = MessageInput("test".into());
        let debug_str = format!("{:?}", msg);
        assert!(debug_str.contains("MessageInput"));
    }

    #[test]
    fn test_channel_view_message_clone() {
        let msg = PrepareReply(42);
        let cloned = msg.clone();
        assert!(matches!(cloned, PrepareReply(id) if id == 42));
    }

    #[test]
    fn test_message_unseen() {
        let mut channel_view = ChannelView::new(ChannelId::Channel(0), 0);
        let message = ChannelViewEntry::new(42, 1, NewTextMessage("test".into()), now_secs());
        let _ = channel_view.new_message(message, &HistoryLength::All);

        // Mark seen first
        let _ = channel_view.update(MessageSeen(42));
        assert!(
            channel_view
                .entries
                .get(&42)
                .expect("Entry with id 42 should exist after adding message")
                .seen()
        );

        // MessageUnseen should not change anything (currently a no-op)
        let _ = channel_view.update(super::ChannelViewMessage::MessageUnseen(42));
        // Still seen (MessageUnseen is a no-op)
        assert!(
            channel_view
                .entries
                .get(&42)
                .expect("Entry with id 42 should still exist after MessageUnseen")
                .seen()
        );
    }

    #[test]
    fn test_pick_channel() {
        let mut channel_view = ChannelView::new(ChannelId::Channel(0), 0);
        let _task = channel_view.update(super::ChannelViewMessage::PickChannel(Some(
            ChannelId::Channel(1),
        )));
        // Should return a task, not panic
    }

    #[test]
    fn test_pick_channel_none() {
        let mut channel_view = ChannelView::new(ChannelId::Channel(0), 0);
        let _task = channel_view.update(super::ChannelViewMessage::PickChannel(None));
        // Should return a task, not panic
    }

    #[test]
    fn test_reply_with_emoji() {
        let mut channel_view = ChannelView::new(ChannelId::Channel(0), 0);
        let _task = channel_view.update(super::ChannelViewMessage::ReplyWithEmoji(
            42,
            "👍".into(),
            ChannelId::Channel(0),
        ));
        // Should return a task, not panic
    }

    #[test]
    fn test_share_meshchat() {
        let mut channel_view = ChannelView::new(ChannelId::Channel(0), 0);
        assert!(channel_view.message.is_empty());

        let _task = channel_view.update(super::ChannelViewMessage::ShareMeshChat);
        assert!(channel_view.message.contains("MeshChat"));
        assert!(channel_view.message.contains("github.com"));
    }

    #[test]
    fn test_unread_count_with_position_hidden() {
        let mut channel_view = ChannelView::new(ChannelId::Channel(0), 0);

        // Add a text message
        let text_msg = ChannelViewEntry::new(1, 1, NewTextMessage("test".into()), now_secs());
        let _ = channel_view.new_message(text_msg, &HistoryLength::All);

        // Add a position message
        let pos_msg = ChannelViewEntry::new(
            2,
            1,
            crate::channel_view_entry::MCMessage::PositionMessage(crate::MCPosition {
                latitude: 0.0,
                longitude: 0.0,
                altitude: None,
                time: 0,
                location_source: 0,
                altitude_source: 0,
                timestamp: 0,
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
            }),
            now_secs(),
        );
        let _ = channel_view.new_message(pos_msg, &HistoryLength::All);

        // With position updates shown, should be 2 unread
        assert_eq!(channel_view.unread_count(true, true), 2);

        // With position updates hidden, should be 1 unread
        assert_eq!(channel_view.unread_count(false, true), 1);
    }

    #[test]
    fn test_unread_count_with_user_hidden() {
        let mut channel_view = ChannelView::new(ChannelId::Channel(0), 0);

        // Add a text message
        let text_msg = ChannelViewEntry::new(1, 1, NewTextMessage("test".into()), now_secs());
        let _ = channel_view.new_message(text_msg, &HistoryLength::All);

        // Add a user message
        let user_msg = ChannelViewEntry::new(
            2,
            1,
            crate::channel_view_entry::MCMessage::UserMessage(crate::MCUser {
                id: "test".into(),
                long_name: "Test".into(),
                short_name: "T".into(),
                hw_model_str: "".into(),
                hw_model: 0,
                is_licensed: false,
                role_str: "".into(),
                role: 0,
                public_key: vec![],
                is_unmessagable: false,
            }),
            now_secs(),
        );
        let _ = channel_view.new_message(user_msg, &HistoryLength::All);

        // With user updates shown, should be 2 unread
        assert_eq!(channel_view.unread_count(true, true), 2);

        // With user updates hidden, should be 1 unread
        assert_eq!(channel_view.unread_count(true, false), 1);
    }

    #[test]
    fn test_emoji_reply_to_nonexistent_message() {
        let mut channel_view = ChannelView::new(ChannelId::Channel(0), 0);

        // Try to add emoji reply to message that doesn't exist
        let emoji_reply = ChannelViewEntry::new(2, 200, EmojiReply(999, "👍".into()), now_secs());
        let _ = channel_view.new_message(emoji_reply, &HistoryLength::All);

        // Should still have 0 entries (emoji reply doesn't create new entry)
        assert_eq!(channel_view.entries.len(), 0);
    }

    #[test]
    fn test_message_seen_nonexistent() {
        let mut channel_view = ChannelView::new(ChannelId::Channel(0), 0);
        // Should not panic when marking nonexistent message as seen
        let _ = channel_view.update(MessageSeen(999));
    }

    #[test]
    fn test_trim_history_all() {
        let mut channel_view = ChannelView::new(ChannelId::Channel(0), 0);

        // Add many messages
        for i in 0..10 {
            let message = ChannelViewEntry::new(i, 1, NewTextMessage(format!("msg {}", i)), i);
            let _ = channel_view.new_message(message, &HistoryLength::All);
        }

        // All messages should be kept
        assert_eq!(channel_view.entries.len(), 10);
    }

    #[test]
    fn test_multiple_emoji_replies() {
        let mut channel_view = ChannelView::new(ChannelId::Channel(0), 0);

        // Add the original message
        let original = ChannelViewEntry::new(1, 100, NewTextMessage("Hello".into()), now_secs());
        let _ = channel_view.new_message(original, &HistoryLength::All);

        // Add multiple emoji replies
        let emoji1 = ChannelViewEntry::new(2, 200, EmojiReply(1, "👍".into()), now_secs());
        let _ = channel_view.new_message(emoji1, &HistoryLength::All);

        let emoji2 = ChannelViewEntry::new(3, 300, EmojiReply(1, "❤️".into()), now_secs());
        let _ = channel_view.new_message(emoji2, &HistoryLength::All);

        let emoji3 = ChannelViewEntry::new(4, 200, EmojiReply(1, "👍".into()), now_secs());
        let _ = channel_view.new_message(emoji3, &HistoryLength::All);

        // Should still only have 1 entry
        assert_eq!(channel_view.entries.len(), 1);

        // Original should have multiple emojis
        let original_entry = channel_view
            .entries
            .get(&1)
            .expect("Original message with id 1 should exist after adding emoji replies");
        assert!(original_entry.emojis().contains_key("👍"));
        assert!(original_entry.emojis().contains_key("❤️"));
    }

    #[test]
    fn test_text_message_reply() {
        let mut channel_view = ChannelView::new(ChannelId::Channel(0), 0);

        // Add the original message
        let original = ChannelViewEntry::new(1, 100, NewTextMessage("Hello".into()), now_secs());
        let _ = channel_view.new_message(original, &HistoryLength::All);

        // Add a text reply
        let reply = ChannelViewEntry::new(
            2,
            200,
            crate::channel_view_entry::MCMessage::TextMessageReply(1, "Hi there!".into()),
            now_secs(),
        );
        let _ = channel_view.new_message(reply, &HistoryLength::All);

        // Should have 2 entries
        assert_eq!(channel_view.entries.len(), 2);
    }

    #[test]
    fn test_alert_message() {
        let mut channel_view = ChannelView::new(ChannelId::Channel(0), 0);

        let alert = ChannelViewEntry::new(
            1,
            100,
            crate::channel_view_entry::MCMessage::AlertMessage("Emergency!".into()),
            now_secs(),
        );
        let _ = channel_view.new_message(alert, &HistoryLength::All);

        assert_eq!(channel_view.entries.len(), 1);
    }

    // View function tests

    #[test]
    fn test_empty_view() {
        let _element = ChannelView::empty_view();
        // Should not panic and return Element for empty channel
    }

    #[test]
    fn test_day_separator_today() {
        use chrono::Local;
        let now = Local::now();
        let _element = ChannelView::day_separator(&now);
        // Should show "Today" for current day
    }

    #[test]
    fn test_day_separator_past_day_same_week() {
        use chrono::{Duration, Local};
        let past = Local::now() - Duration::days(2);
        let _element = ChannelView::day_separator(&past);
        // Should show day name only
    }

    #[test]
    fn test_day_separator_past_week() {
        use chrono::{Duration, Local};
        let past = Local::now() - Duration::days(10);
        let _element = ChannelView::day_separator(&past);
        // Should show day name with month and date
    }

    #[test]
    fn test_day_separator_past_year() {
        use chrono::{Duration, Local};
        let past = Local::now() - Duration::days(400);
        let _element = ChannelView::day_separator(&past);
        // Should show full date with year
    }

    #[test]
    fn test_send_button_empty_message() {
        let channel_view = ChannelView::new(ChannelId::Channel(0), 100);
        let _button = channel_view.send_button();
        // Button should be disabled when message is empty
    }

    #[test]
    fn test_send_button_with_message() {
        let mut channel_view = ChannelView::new(ChannelId::Channel(0), 100);
        let _ = channel_view.update(MessageInput("Hello".into()));
        let _button = channel_view.send_button();
        // Button should be enabled when message is not empty
    }

    #[test]
    fn test_clear_button_empty_message() {
        let channel_view = ChannelView::new(ChannelId::Channel(0), 100);
        let _button = channel_view.clear_button();
        // Button should be disabled when message is empty
    }

    #[test]
    fn test_clear_button_with_message() {
        let mut channel_view = ChannelView::new(ChannelId::Channel(0), 100);
        let _ = channel_view.update(MessageInput("Hello".into()));
        let _button = channel_view.clear_button();
        // Button should be enabled when message is not empty
    }

    #[test]
    fn test_input_box_empty() {
        let channel_view = ChannelView::new(ChannelId::Channel(0), 100);
        let _element = channel_view.input_box();
        // Should not panic
    }

    #[test]
    fn test_input_box_with_message() {
        let mut channel_view = ChannelView::new(ChannelId::Channel(0), 100);
        let _ = channel_view.update(MessageInput("Hello world".into()));
        let _element = channel_view.input_box();
        // Should not panic
    }

    #[test]
    fn test_now_local() {
        let now = ChannelView::now_local();
        // Should return current local time
        assert!(now.timestamp() > 0);
    }

    #[test]
    fn test_replying_to_with_valid_entry() {
        use iced::widget::Column;

        let mut channel_view = ChannelView::new(ChannelId::Channel(0), 100);
        let message = ChannelViewEntry::new(1, 200, NewTextMessage("Original".into()), now_secs());
        let _ = channel_view.new_message(message, &HistoryLength::All);
        let _ = channel_view.update(PrepareReply(1));

        let column: Column<crate::Message> = Column::new();
        let _result_column = channel_view.replying_to(column, &1);
        // Should add reply indicator to the column
    }

    #[test]
    fn test_replying_to_with_invalid_entry() {
        use iced::widget::Column;

        let channel_view = ChannelView::new(ChannelId::Channel(0), 100);
        // No messages in the channel view

        let column: Column<crate::Message> = Column::new();
        let _result_column = channel_view.replying_to(column, &999);
        // Should return column unchanged
    }
}
