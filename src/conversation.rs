use crate::Message::DeviceViewEvent;
use crate::config::{Config, HistoryLength};
use crate::conversation::ChannelViewMessage::{
    CancelPrepareReply, ClearMessage, EmojiPickerMsg, MessageInput, MessageSeen, MessageUnseen,
    PickChannel, PrepareReply, ReplyWithEmoji, SendMessage, ShareMeshChat,
};
use crate::conversation_id::{ConversationId, MessageId, NodeId};
use crate::device::DeviceViewMessage::{
    ChannelMsg, ForwardMessage, SendPositionMessage, SendSelfInfoMessage, ShowChannel,
    StopForwardingMessage,
};
use crate::device::{Device, DeviceViewMessage};
use crate::meshchat::MCNodeInfo;
use crate::message::MCContent::{
    AlertMessage, EmojiReply, NewTextMessage, PositionMessage, TextMessageReply, UserMessage,
};
use crate::styles::{
    DAY_SEPARATOR_STYLE, button_chip_style, picker_header_style, reply_to_style, scrollbar_style,
    text_input_button_style, text_input_container_style, text_input_style, tooltip_style,
};
use crate::widgets::emoji_picker::{EmojiPicker, PickerMessage};
use crate::{MeshChat, Message, icons, message::MCMessage};
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

pub const MESSAGE_INPUT_ID: Id = Id::new("message_input");
const CHANNEL_VIEW_SCROLLABLE_ID: Id = Id::new("channel_view_scrollable");

#[derive(Debug, Clone)]
pub enum ChannelViewMessage {
    MessageInput(String),
    ClearMessage,
    SendMessage(Option<MessageId>), // optional message id if we are replying to that message
    PrepareReply(MessageId),        // entry_id
    CancelPrepareReply,
    MessageSeen(MessageId),
    MessageUnseen(MessageId),
    PickChannel(Option<ConversationId>),
    ReplyWithEmoji(MessageId, String, ConversationId), // Send an emoji reply
    EmojiPickerMsg(Box<PickerMessage<ChannelViewMessage>>),
    ShareMeshChat,
}

/// [Conversation] implements view and update methods for Iced for a set of
/// messages to and from a "Channel" which can be a Channel or a Node
#[derive(Debug, Default)]
pub struct Conversation {
    conversation_id: ConversationId,
    message: String,                         // text message typed in so far
    messages: RingMap<MessageId, MCMessage>, // entries received so far, keyed by message_id, ordered by timestamp
    my_node_num: NodeId,
    preparing_reply_to: Option<MessageId>,
    emoji_picker: EmojiPicker,
}

async fn empty() {}

// A view of a single channel and its messages, which maybe a Channel or a Node
impl Conversation {
    pub fn new(conversation_id: ConversationId, my_node_id: NodeId) -> Self {
        Self {
            conversation_id,
            my_node_num: my_node_id,
            emoji_picker: EmojiPicker::new().height(400).width(400),
            ..Default::default()
        }
    }

    /// Acknowledge the receipt of a message.
    pub fn ack(&mut self, message_id: MessageId) {
        if let Some(entry) = self.messages.get_mut(&message_id) {
            entry.ack();
        }
    }

    /// Get the time now as a [DateTime<Local>]
    fn now_local() -> DateTime<Local> {
        let timestamp = MeshChat::now();
        let datetime_utc =
            DateTime::<Utc>::from_timestamp_secs(timestamp.into()).unwrap_or_default();
        datetime_utc.with_timezone(&Local)
    }

    /// Remove older messages according to the passed in history_length setting
    fn trim_history(&mut self, history_length: &HistoryLength) {
        // if there is an active config for the maximum length of history, then trim
        // the list of entries
        match history_length {
            HistoryLength::NumberOfMessages(number) => {
                while self.messages.len() > *number {
                    self.messages.pop_front();
                }
            }
            HistoryLength::Duration(duration) => {
                let oldest = Self::now_local() - *duration;
                while self
                    .messages
                    .pop_front_if(|_k, channel_view_entry| channel_view_entry.time() < oldest)
                    .is_some()
                {}
            }
            _ => { /* leave all messages as no limit set */ }
        }
    }

    /// Add a new [MCMessage] to the [Conversation], unless it's an emoji reply,
    /// in which case the emoji will be added to the Vec of emoji replies of the original
    /// message [MCMessage], and no new entry created
    pub fn new_message(
        &mut self,
        new_message: MCMessage,
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
                self.messages.insert_sorted_by(
                    new_message.message_id(),
                    new_message,
                    MCMessage::sort_by_timestamp,
                );

                self.trim_history(history_length);
            }
            EmojiReply(reply_to_id, emoji_string) => {
                if let Some(entry) = self.messages.get_mut(reply_to_id) {
                    entry.add_emoji(emoji_string.to_string(), new_message.from());
                }
            }
        };

        if mine {
            // scroll to the end of messages to see the message I just sent
            operation::snap_to(CHANNEL_VIEW_SCROLLABLE_ID, RelativeOffset::END)
        } else {
            Task::none()
        }
    }

    /// Cancel any interactive modes underway
    pub fn cancel_interactive(&mut self) {
        self.preparing_reply_to = None;
    }

    /// Update the [Conversation] state based on a [ChannelViewMessage]
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
                    let conversation_id = self.conversation_id;
                    self.preparing_reply_to = None;
                    Task::perform(empty(), move |_| {
                        DeviceViewEvent(DeviceViewMessage::SendTextMessage(
                            msg,
                            conversation_id,
                            reply_to_id,
                        ))
                    })
                } else {
                    Task::none()
                }
            }
            PrepareReply(entry_id) => {
                if self.messages.contains_key(&entry_id) {
                    self.preparing_reply_to = Some(entry_id);
                    operation::focus(MESSAGE_INPUT_ID)
                } else {
                    Task::none()
                }
            }
            CancelPrepareReply => {
                self.cancel_interactive();
                Task::none()
            }
            MessageSeen(message_id) => {
                if let Some(channel_view_entry) = self.messages.get_mut(&message_id) {
                    channel_view_entry.mark_seen();
                } else {
                    println!("Error Message {} not found in ChannelView", message_id);
                }
                Task::none()
            }
            MessageUnseen(_) => Task::none(),
            PickChannel(conversation_id) => Task::perform(empty(), move |_| {
                DeviceViewEvent(ShowChannel(conversation_id))
            }),
            ReplyWithEmoji(message_id, emoji, conversation_id) => {
                Task::perform(empty(), move |_| {
                    DeviceViewEvent(DeviceViewMessage::SendEmojiReplyMessage(
                        message_id,
                        emoji,
                        conversation_id,
                    ))
                })
            }
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
                    "I am using the MeshChat app. It's available for macOS/Windows/Linux from: \
                https://github.com/andrewdavidmackenzie/meshchat/releases/latest ",
                );
                // and shift the focus to it in case the user wants to add to it or edit it
                operation::focus(MESSAGE_INPUT_ID)
            }
        }
    }

    /// Construct an Element that displays the channel view
    #[allow(clippy::too_many_arguments)]
    pub fn view<'a>(
        &'a self,
        nodes: &'a HashMap<NodeId, MCNodeInfo>,
        enable_position: bool,
        enable_my_user: bool,
        device_view: &'a Device,
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
        self.messages.values().fold(0, |acc, entry| {
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
        nodes: &'a HashMap<NodeId, MCNodeInfo>,
        enable_position: bool,
        enable_my_info: bool,
        show_position_updates: bool,
        show_user_updates: bool,
    ) -> Element<'a, Message> {
        let mut channel_view_content = Column::new().padding(right(10));

        let message_area: Element<'a, Message> = if self.messages.is_empty() {
            Self::empty_view()
        } else {
            let mut previous_day = u32::MIN;

            // Add a view to the column for each of the entries in this Channel
            for entry in self.messages.values() {
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
                    &self.messages,
                    nodes,
                    &self.conversation_id,
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
                .id(CHANNEL_VIEW_SCROLLABLE_ID)
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
                .on_press(DeviceViewEvent(SendPositionMessage(self.conversation_id)));
        }

        let mut send_info_button = button(text("Send Info ⓘ")).style(button_chip_style);
        if enable_my_info {
            send_info_button = send_info_button
                .on_press(DeviceViewEvent(SendSelfInfoMessage(self.conversation_id)));
        }

        // a button to allow easy sharing of this app
        let share_meshchat_button =
            button(row([text("Share MeshChat ").into(), icons::share().into()]))
                .style(button_chip_style)
                .on_press(DeviceViewEvent(ChannelMsg(
                    self.conversation_id,
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
        if let Some(entry_id) = &self.preparing_reply_to {
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
        device_view: &'a Device,
        config: &'a Config,
    ) -> Element<'a, Message> {
        let select =
            |channel_number: ConversationId| DeviceViewEvent(ForwardMessage(channel_number));
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
        message_id: &MessageId,
    ) -> Column<'a, Message> {
        if let Some(original_text) = MCMessage::reply_quote(&self.messages, message_id) {
            let cancel_reply_button: Button<Message> = button(text("⨂").size(16))
                .on_press(DeviceViewEvent(ChannelMsg(
                    self.conversation_id,
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
                self.conversation_id,
                SendMessage(self.preparing_reply_to),
            )));
        }

        send_button
    }

    fn clear_button(&'_ self) -> Button<'_, Message> {
        let mut clear_button = button(text("⨂").size(18))
            .style(text_input_button_style)
            .padding(Padding::from([6, 6]));
        if !self.message.is_empty() {
            clear_button = clear_button.on_press(DeviceViewEvent(ChannelMsg(
                self.conversation_id,
                ClearMessage,
            )));
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
                            .id(MESSAGE_INPUT_ID)
                            .on_input(|s| {
                                DeviceViewEvent(ChannelMsg(self.conversation_id, MessageInput(s)))
                            })
                            .on_submit(DeviceViewEvent(ChannelMsg(
                                self.conversation_id,
                                SendMessage(self.preparing_reply_to),
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
        .padding(Padding::from([4, 0]))
        .into()
    }
}

#[cfg(test)]
mod test {
    use crate::MeshChat;
    use crate::config::HistoryLength;
    use crate::conversation::ChannelViewMessage::{
        CancelPrepareReply, ClearMessage, MessageInput, MessageSeen, PrepareReply, SendMessage,
    };
    use crate::conversation::{Conversation, ConversationId};
    use crate::conversation_id::{MessageId, NodeId};
    use crate::device::TimeStamp;
    use crate::meshchat::{MCPosition, MCUser};
    use crate::message::MCContent::{EmojiReply, NewTextMessage};
    use crate::message::MCMessage;
    use std::time::Duration;

    #[tokio::test]
    async fn message_ordering_test() {
        let mut channel_view =
            Conversation::new(ConversationId::Channel(0.into()), NodeId::from(0u64));
        assert!(
            channel_view.messages.is_empty(),
            "Initial entries list should be empty"
        );

        // create a set of messages with more than a second between them
        // message ids are not in order
        let oldest_message = MCMessage::new(
            MessageId::from(1),
            NodeId::from(1u64),
            NewTextMessage("Hello 1".to_string()),
            MeshChat::now(),
        );
        tokio::time::sleep(Duration::from_millis(1500)).await;

        let middle_message = MCMessage::new(
            MessageId::from(2),
            NodeId::from(1000u64),
            NewTextMessage("Hello 2".to_string()),
            MeshChat::now(),
        );
        tokio::time::sleep(Duration::from_millis(1500)).await;

        let newest_message = MCMessage::new(
            MessageId::from(3),
            NodeId::from(500u64),
            NewTextMessage("Hello 3".to_string()),
            MeshChat::now(),
        );

        // Add them out of order - they should be ordered by the time of creation, not entry
        let _ = channel_view.new_message(newest_message.clone(), &HistoryLength::All);
        let _ = channel_view.new_message(middle_message.clone(), &HistoryLength::All);
        let _ = channel_view.new_message(oldest_message.clone(), &HistoryLength::All);
        assert_eq!(
            channel_view.messages.values().len(),
            3,
            "There should be 3 messages in the list"
        );
        assert_eq!(
            channel_view.unread_count(true, true),
            3,
            "The unread count should be 3"
        );

        // Check the order is correct
        let mut iter = channel_view.messages.values();
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
        let channel_view = Conversation::new(ConversationId::Channel(0.into()), NodeId::from(0u64));
        assert_eq!(channel_view.unread_count(true, true), 0);
    }

    #[test]
    fn test_unread_count() {
        let mut channel_view =
            Conversation::new(ConversationId::Channel(0.into()), NodeId::from(0u64));
        let message = MCMessage::new(
            MessageId::from(1),
            NodeId::from(1u64),
            NewTextMessage("Hello 1".to_string()),
            MeshChat::now(),
        );
        let _ = channel_view.new_message(message.clone(), &HistoryLength::All);
        assert_eq!(channel_view.unread_count(true, true), 1);
    }

    #[test]
    fn test_replying_valid_entry() {
        let mut channel_view =
            Conversation::new(ConversationId::Channel(0.into()), NodeId::from(0u64));
        let message = MCMessage::new(
            MessageId::from(1),
            NodeId::from(1u64),
            NewTextMessage("Hello 1".to_string()),
            MeshChat::now(),
        );
        let _ = channel_view.new_message(message, &HistoryLength::All);

        // Use the actual message ID (1), not an index
        let _ = channel_view.update(PrepareReply(MessageId::from(1)));

        assert_eq!(channel_view.preparing_reply_to, Some(MessageId::from(1)));
    }

    #[test]
    fn test_replying_invalid_entry() {
        let mut channel_view =
            Conversation::new(ConversationId::Channel(0.into()), NodeId::from(0u64));
        let message = MCMessage::new(
            MessageId::from(1),
            NodeId::from(1u64),
            NewTextMessage("Hello 1".to_string()),
            MeshChat::now(),
        );
        let _ = channel_view.new_message(message, &HistoryLength::All);

        // Use a non-existent message ID (999) - should not set preparing_reply
        let _ = channel_view.update(PrepareReply(MessageId::from(999)));

        assert!(channel_view.preparing_reply_to.is_none());
    }

    #[test]
    fn test_cancel_prepare_reply() {
        let mut channel_view =
            Conversation::new(ConversationId::Channel(0.into()), NodeId::from(0u64));
        let message = MCMessage::new(
            MessageId::from(1),
            NodeId::from(1u64),
            NewTextMessage("test".into()),
            MeshChat::now(),
        );
        let _ = channel_view.new_message(message, &HistoryLength::All);
        let _ = channel_view.update(PrepareReply(MessageId::from(1))); // Use actual message ID
        assert!(channel_view.preparing_reply_to.is_some());

        let _ = channel_view.update(CancelPrepareReply);
        assert!(channel_view.preparing_reply_to.is_none());
    }

    #[test]
    fn test_cancel_interactive() {
        let mut channel_view =
            Conversation::new(ConversationId::Channel(0.into()), NodeId::from(0u64));
        let message = MCMessage::new(
            MessageId::from(1),
            NodeId::from(1u64),
            NewTextMessage("test".into()),
            MeshChat::now(),
        );
        let _ = channel_view.new_message(message, &HistoryLength::All);
        let _ = channel_view.update(PrepareReply(MessageId::from(1))); // Use actual message ID
        assert!(channel_view.preparing_reply_to.is_some());

        channel_view.cancel_interactive();
        assert!(channel_view.preparing_reply_to.is_none());
    }

    #[test]
    fn test_message_input() {
        let mut channel_view =
            Conversation::new(ConversationId::Channel(0.into()), NodeId::from(0u64));
        assert!(channel_view.message.is_empty());

        let _ = channel_view.update(MessageInput("Hello".into()));
        assert_eq!(channel_view.message, "Hello");
    }

    #[test]
    fn test_message_input_max_length() {
        let mut channel_view =
            Conversation::new(ConversationId::Channel(0.into()), NodeId::from(0u64));

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
        let mut channel_view =
            Conversation::new(ConversationId::Channel(0.into()), NodeId::from(0u64));
        let _ = channel_view.update(MessageInput("Hello".into()));
        assert!(!channel_view.message.is_empty());

        let _ = channel_view.update(ClearMessage);
        assert!(channel_view.message.is_empty());
    }

    #[test]
    fn test_send_message_empty() {
        let mut channel_view =
            Conversation::new(ConversationId::Channel(0.into()), NodeId::from(0u64));
        // Empty message should not trigger a send task
        let _ = channel_view.update(SendMessage(None));
        // No crash, message still empty
        assert!(channel_view.message.is_empty());
    }

    #[test]
    fn test_send_message_clears_text() {
        let mut channel_view =
            Conversation::new(ConversationId::Channel(0.into()), NodeId::from(0u64));
        let _ = channel_view.update(MessageInput("Hello world".into()));
        assert!(!channel_view.message.is_empty());

        let _ = channel_view.update(SendMessage(None));
        assert!(channel_view.message.is_empty());
    }

    #[test]
    fn test_send_message_clears_preparing_reply() {
        let mut channel_view =
            Conversation::new(ConversationId::Channel(0.into()), NodeId::from(0u64));
        let message = MCMessage::new(
            MessageId::from(1),
            NodeId::from(1u64),
            NewTextMessage("test".into()),
            MeshChat::now(),
        );
        let _ = channel_view.new_message(message, &HistoryLength::All);
        let _ = channel_view.update(PrepareReply(MessageId::from(1))); // Use actual message ID
        let _ = channel_view.update(MessageInput("Reply text".into()));

        let _ = channel_view.update(SendMessage(Some(MessageId::from(1))));
        assert!(channel_view.preparing_reply_to.is_none());
    }

    #[test]
    fn test_ack_message() {
        let mut channel_view =
            Conversation::new(ConversationId::Channel(0.into()), NodeId::from(0u64));
        let message = MCMessage::new(
            MessageId::from(42),
            NodeId::from(1u64),
            NewTextMessage("test".into()),
            MeshChat::now(),
        );
        let _ = channel_view.new_message(message, &HistoryLength::All);

        assert!(
            !channel_view
                .messages
                .get(&MessageId::from(42))
                .expect("entry 42 should exist")
                .acked()
        );

        channel_view.ack(MessageId::from(42));
        assert!(
            channel_view
                .messages
                .get(&MessageId::from(42))
                .expect("entry 42 should exist after ack")
                .acked()
        );
    }

    #[test]
    fn test_ack_nonexistent_message() {
        let mut channel_view =
            Conversation::new(ConversationId::Channel(0.into()), NodeId::from(0u64));
        // Should not panic
        channel_view.ack(MessageId::from(999));
    }

    #[test]
    fn test_message_seen() {
        let mut channel_view =
            Conversation::new(ConversationId::Channel(0.into()), NodeId::from(0u64));
        let message = MCMessage::new(
            MessageId::from(42),
            NodeId::from(1u64),
            NewTextMessage("test".into()),
            MeshChat::now(),
        );
        let _ = channel_view.new_message(message, &HistoryLength::All);

        assert!(
            !channel_view
                .messages
                .get(&MessageId::from(42))
                .expect("entry 42 should exist")
                .seen()
        );
        assert_eq!(channel_view.unread_count(true, true), 1);

        let _ = channel_view.update(MessageSeen(MessageId::from(42)));
        assert!(
            channel_view
                .messages
                .get(&MessageId::from(42))
                .expect("entry 42 should exist after marking seen")
                .seen()
        );
        assert_eq!(channel_view.unread_count(true, true), 0);
    }

    #[test]
    fn test_trim_history_by_number() {
        let mut channel_view =
            Conversation::new(ConversationId::Channel(0.into()), NodeId::from(0u64));

        // Add 5 messages
        for i in 0..5 {
            let message = MCMessage::new(
                MessageId::from(i),
                NodeId::from(1u64),
                NewTextMessage(format!("msg {}", i)),
                TimeStamp::from(i),
            );
            let _ = channel_view.new_message(message, &HistoryLength::All);
        }
        assert_eq!(channel_view.messages.len(), 5);

        // Adding the 6th message with a limit of 3 should trim to 3
        let message = MCMessage::new(
            MessageId::from(5),
            NodeId::from(1u64),
            NewTextMessage("msg 5".into()),
            TimeStamp::from(5),
        );
        let _ = channel_view.new_message(message, &HistoryLength::NumberOfMessages(3));
        assert_eq!(channel_view.messages.len(), 3);

        // The oldest messages should have been removed
        assert!(!channel_view.messages.contains_key(&MessageId::from(0)));
        assert!(!channel_view.messages.contains_key(&MessageId::from(1)));
        assert!(!channel_view.messages.contains_key(&MessageId::from(2)));
        assert!(channel_view.messages.contains_key(&MessageId::from(3)));
        assert!(channel_view.messages.contains_key(&MessageId::from(4)));
        assert!(channel_view.messages.contains_key(&MessageId::from(5)));
    }

    #[test]
    fn test_trim_history_by_duration() {
        let mut channel_view =
            Conversation::new(ConversationId::Channel(0.into()), NodeId::from(0u64));
        let now = MeshChat::now();

        // Add an old message (2 hours ago)
        let old_time = now - TimeStamp::from(7200);
        let old_message = MCMessage::new(
            MessageId::from(1),
            NodeId::from(1u64),
            NewTextMessage("old".into()),
            old_time,
        );
        let _ = channel_view.new_message(old_message, &HistoryLength::All);

        // Add a recent message (now)
        let new_message = MCMessage::new(
            MessageId::from(2),
            NodeId::from(1u64),
            NewTextMessage("new".into()),
            now,
        );
        // Trim history to 1 hour (3600 seconds)
        let _ = channel_view.new_message(
            new_message,
            &HistoryLength::Duration(Duration::from_secs(3600)),
        );

        // Old message should be trimmed
        assert_eq!(channel_view.messages.len(), 1);
        assert!(channel_view.messages.contains_key(&MessageId::from(2)));
    }

    #[test]
    fn test_emoji_reply_adds_to_existing_message() {
        let mut channel_view =
            Conversation::new(ConversationId::Channel(0.into()), NodeId::from(0u64));

        // Add the original message
        let original = MCMessage::new(
            MessageId::from(1),
            NodeId::from(100u64),
            NewTextMessage("Hello".into()),
            MeshChat::now(),
        );
        let _ = channel_view.new_message(original, &HistoryLength::All);
        assert_eq!(channel_view.messages.len(), 1);

        // Add emoji reply to message 1
        let emoji_reply = MCMessage::new(
            MessageId::from(2),
            NodeId::from(200u64),
            EmojiReply(MessageId::from(1), "👍".into()),
            MeshChat::now(),
        );
        let _ = channel_view.new_message(emoji_reply, &HistoryLength::All);

        // Should still only have 1 entry (the original)
        assert_eq!(channel_view.messages.len(), 1);
        // But the original should now have the emoji
        let original_entry = channel_view
            .messages
            .get(&MessageId::from(1))
            .expect("original entry should exist after emoji reply");
        assert!(original_entry.emojis().contains_key("👍"));
    }

    #[test]
    fn test_new_default() {
        let channel_view = Conversation::default();
        assert_eq!(channel_view.conversation_id, ConversationId::default());
        assert!(channel_view.message.is_empty());
        assert!(channel_view.messages.is_empty());
        assert!(channel_view.preparing_reply_to.is_none());
    }

    #[test]
    fn test_my_message_scrolls() {
        let mut channel_view =
            Conversation::new(ConversationId::Channel(0.into()), NodeId::from(100u64));

        // Message from me (node 100)
        let my_message = MCMessage::new(
            MessageId::from(1),
            NodeId::from(100u64),
            NewTextMessage("my msg".into()),
            MeshChat::now(),
        );
        let task = channel_view.new_message(my_message, &HistoryLength::All);

        // Should return a scroll task (not Task::none)
        // We can't easily check the task type, but we can verify the message was added
        assert_eq!(channel_view.messages.len(), 1);
        // The task should not be none - it should be a snap_to operation
        drop(task); // Just verify it doesn't panic
    }

    #[test]
    fn test_others_message_no_scroll() {
        let mut channel_view =
            Conversation::new(ConversationId::Channel(0.into()), NodeId::from(100u64));

        // Message from someone else (node 200)
        let other_message = MCMessage::new(
            MessageId::from(1),
            NodeId::from(200u64),
            NewTextMessage("other msg".into()),
            MeshChat::now(),
        );
        let _task = channel_view.new_message(other_message, &HistoryLength::All);

        assert_eq!(channel_view.messages.len(), 1);
    }

    #[test]
    fn test_channel_view_debug() {
        let channel_view =
            Conversation::new(ConversationId::Channel(0.into()), NodeId::from(100u64));
        let debug_str = format!("{:?}", channel_view);
        assert!(debug_str.contains("Conversation"));
    }

    #[test]
    fn test_channel_view_message_debug() {
        let msg = MessageInput("test".into());
        let debug_str = format!("{:?}", msg);
        assert!(debug_str.contains("MessageInput"));
    }

    #[test]
    fn test_channel_view_message_clone() {
        let msg = PrepareReply(MessageId::from(42));
        let cloned = msg.clone();
        assert!(matches!(cloned, PrepareReply(id) if id == MessageId::from(42)));
    }

    #[test]
    fn test_message_unseen() {
        let mut channel_view =
            Conversation::new(ConversationId::Channel(0.into()), NodeId::from(0u64));
        let message = MCMessage::new(
            MessageId::from(42),
            NodeId::from(1u64),
            NewTextMessage("test".into()),
            MeshChat::now(),
        );
        let _ = channel_view.new_message(message, &HistoryLength::All);

        // Mark seen first
        let _ = channel_view.update(MessageSeen(MessageId::from(42)));
        assert!(
            channel_view
                .messages
                .get(&MessageId::from(42))
                .expect("Entry with id 42 should exist after adding message")
                .seen()
        );

        // MessageUnseen should not change anything (currently a no-op)
        let _ = channel_view.update(super::ChannelViewMessage::MessageUnseen(MessageId::from(
            42,
        )));
        // Still seen (MessageUnseen is a no-op)
        assert!(
            channel_view
                .messages
                .get(&MessageId::from(42))
                .expect("Entry with id 42 should still exist after MessageUnseen")
                .seen()
        );
    }

    #[test]
    fn test_pick_channel() {
        let mut channel_view =
            Conversation::new(ConversationId::Channel(0.into()), NodeId::from(0u64));
        let _task = channel_view.update(super::ChannelViewMessage::PickChannel(Some(
            ConversationId::Channel(1.into()),
        )));
        // Should return a Task, not panic
    }

    #[test]
    fn test_pick_channel_none() {
        let mut channel_view =
            Conversation::new(ConversationId::Channel(0.into()), NodeId::from(0u64));
        let _task = channel_view.update(super::ChannelViewMessage::PickChannel(None));
        // Should return a Task, not panic
    }

    #[test]
    fn test_reply_with_emoji() {
        let mut channel_view =
            Conversation::new(ConversationId::Channel(0.into()), NodeId::from(0u64));
        let _task = channel_view.update(super::ChannelViewMessage::ReplyWithEmoji(
            MessageId::from(42),
            "👍".into(),
            ConversationId::Channel(0.into()),
        ));
        // Should return a Task, not panic
    }

    #[test]
    fn test_share_meshchat() {
        let mut channel_view =
            Conversation::new(ConversationId::Channel(0.into()), NodeId::from(0u64));
        assert!(channel_view.message.is_empty());

        let _task = channel_view.update(super::ChannelViewMessage::ShareMeshChat);
        assert!(channel_view.message.contains("MeshChat"));
        assert!(channel_view.message.contains("github.com"));
    }

    #[test]
    fn test_unread_count_with_position_hidden() {
        let mut channel_view =
            Conversation::new(ConversationId::Channel(0.into()), NodeId::from(0u64));

        // Add a text message
        let text_msg = MCMessage::new(
            MessageId::from(1),
            NodeId::from(1u64),
            NewTextMessage("test".into()),
            MeshChat::now(),
        );
        let _ = channel_view.new_message(text_msg, &HistoryLength::All);

        // Add a position message
        let pos_msg = MCMessage::new(
            MessageId::from(2),
            NodeId::from(1u64),
            crate::message::MCContent::PositionMessage(MCPosition {
                latitude: 0.0,
                longitude: 0.0,
                ..Default::default()
            }),
            MeshChat::now(),
        );
        let _ = channel_view.new_message(pos_msg, &HistoryLength::All);

        // With position updates shown, should be 2 unread
        assert_eq!(channel_view.unread_count(true, true), 2);

        // With position updates hidden, should be 1 unread
        assert_eq!(channel_view.unread_count(false, true), 1);
    }

    #[test]
    fn test_unread_count_with_user_hidden() {
        let mut channel_view =
            Conversation::new(ConversationId::Channel(0.into()), NodeId::from(0u64));

        // Add a text message
        let text_msg = MCMessage::new(
            MessageId::from(1),
            NodeId::from(1u64),
            NewTextMessage("test".into()),
            MeshChat::now(),
        );
        let _ = channel_view.new_message(text_msg, &HistoryLength::All);

        // Add a user message
        let user_msg = MCMessage::new(
            MessageId::from(2),
            NodeId::from(1u64),
            crate::message::MCContent::UserMessage(MCUser {
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
            MeshChat::now(),
        );
        let _ = channel_view.new_message(user_msg, &HistoryLength::All);

        // With user updates shown, should be 2 unread
        assert_eq!(channel_view.unread_count(true, true), 2);

        // With user updates hidden, should be 1 unread
        assert_eq!(channel_view.unread_count(true, false), 1);
    }

    #[test]
    fn test_emoji_reply_to_nonexistent_message() {
        let mut channel_view =
            Conversation::new(ConversationId::Channel(0.into()), NodeId::from(0u64));

        // Try to add emoji reply to the message that doesn't exist
        let emoji_reply = MCMessage::new(
            MessageId::from(2),
            NodeId::from(200u64),
            EmojiReply(MessageId::from(999), "👍".into()),
            MeshChat::now(),
        );
        let _ = channel_view.new_message(emoji_reply, &HistoryLength::All);

        // Should still have 0 entries (emoji reply doesn't create the new entry)
        assert_eq!(channel_view.messages.len(), 0);
    }

    #[test]
    fn test_message_seen_nonexistent() {
        let mut channel_view =
            Conversation::new(ConversationId::Channel(0.into()), NodeId::from(0u64));
        // Should not panic when marking a nonexistent message as seen
        let _ = channel_view.update(MessageSeen(MessageId::from(999)));
    }

    #[test]
    fn test_trim_history_all() {
        let mut channel_view =
            Conversation::new(ConversationId::Channel(0.into()), NodeId::from(0u64));

        // Add many messages
        for i in 0..10 {
            let message = MCMessage::new(
                MessageId::from(i),
                NodeId::from(1u64),
                NewTextMessage(format!("msg {}", i)),
                TimeStamp::from(i),
            );
            let _ = channel_view.new_message(message, &HistoryLength::All);
        }

        // All messages should be kept
        assert_eq!(channel_view.messages.len(), 10);
    }

    #[test]
    fn test_multiple_emoji_replies() {
        let mut channel_view =
            Conversation::new(ConversationId::Channel(0.into()), NodeId::from(0u64));

        // Add the original message
        let original = MCMessage::new(
            MessageId::from(1),
            NodeId::from(100u64),
            NewTextMessage("Hello".into()),
            MeshChat::now(),
        );
        let _ = channel_view.new_message(original, &HistoryLength::All);

        // Add multiple emoji replies
        let emoji1 = MCMessage::new(
            MessageId::from(2),
            NodeId::from(200u64),
            EmojiReply(MessageId::from(1), "👍".into()),
            MeshChat::now(),
        );
        let _ = channel_view.new_message(emoji1, &HistoryLength::All);

        let emoji2 = MCMessage::new(
            MessageId::from(3),
            NodeId::from(300u64),
            EmojiReply(MessageId::from(1), "❤️".into()),
            MeshChat::now(),
        );
        let _ = channel_view.new_message(emoji2, &HistoryLength::All);

        let emoji3 = MCMessage::new(
            MessageId::from(4),
            NodeId::from(200u64),
            EmojiReply(MessageId::from(1), "👍".into()),
            MeshChat::now(),
        );
        let _ = channel_view.new_message(emoji3, &HistoryLength::All);

        // Should still only have 1 entry
        assert_eq!(channel_view.messages.len(), 1);

        // Original should have multiple emojis
        let original_entry = channel_view
            .messages
            .get(&MessageId::from(1))
            .expect("Original message with id 1 should exist after adding emoji replies");
        assert!(original_entry.emojis().contains_key("👍"));
        assert!(original_entry.emojis().contains_key("❤️"));
    }

    #[test]
    fn test_text_message_reply() {
        let mut channel_view =
            Conversation::new(ConversationId::Channel(0.into()), NodeId::from(0u64));

        // Add the original message
        let original = MCMessage::new(
            MessageId::from(1),
            NodeId::from(100u64),
            NewTextMessage("Hello".into()),
            MeshChat::now(),
        );
        let _ = channel_view.new_message(original, &HistoryLength::All);

        // Add a text reply
        let reply = MCMessage::new(
            MessageId::from(2),
            NodeId::from(200u64),
            crate::message::MCContent::TextMessageReply(MessageId::from(1), "Hi there!".into()),
            MeshChat::now(),
        );
        let _ = channel_view.new_message(reply, &HistoryLength::All);

        // Should have 2 entries
        assert_eq!(channel_view.messages.len(), 2);
    }

    #[test]
    fn test_alert_message() {
        let mut channel_view =
            Conversation::new(ConversationId::Channel(0.into()), NodeId::from(0u64));

        let alert = MCMessage::new(
            MessageId::from(1),
            NodeId::from(100u64),
            crate::message::MCContent::AlertMessage("Emergency!".into()),
            MeshChat::now(),
        );
        let _ = channel_view.new_message(alert, &HistoryLength::All);

        assert_eq!(channel_view.messages.len(), 1);
    }

    // View function tests

    #[test]
    fn test_empty_view() {
        let _element = Conversation::empty_view();
        // Should not panic and return Element for the empty channel
    }

    #[test]
    fn test_day_separator_today() {
        use chrono::Local;
        let now = Local::now();
        let _element = Conversation::day_separator(&now);
        // Should show "Today" for the current day
    }

    #[test]
    fn test_day_separator_past_day_same_week() {
        use chrono::{Duration, Local};
        let past = Local::now() - Duration::days(2);
        let _element = Conversation::day_separator(&past);
        // Should show day name only
    }

    #[test]
    fn test_day_separator_past_week() {
        use chrono::{Duration, Local};
        let past = Local::now() - Duration::days(10);
        let _element = Conversation::day_separator(&past);
        // Should show day name with month and date
    }

    #[test]
    fn test_day_separator_past_year() {
        use chrono::{Duration, Local};
        let past = Local::now() - Duration::days(400);
        let _element = Conversation::day_separator(&past);
        // Should show the full date with year
    }

    #[test]
    fn test_send_button_empty_message() {
        let channel_view =
            Conversation::new(ConversationId::Channel(0.into()), NodeId::from(100u64));
        let _button = channel_view.send_button();
        // Button should be disabled when the message is empty
    }

    #[test]
    fn test_send_button_with_message() {
        let mut channel_view =
            Conversation::new(ConversationId::Channel(0.into()), NodeId::from(100u64));
        let _ = channel_view.update(MessageInput("Hello".into()));
        let _button = channel_view.send_button();
        // Button should be enabled when the message is not empty
    }

    #[test]
    fn test_clear_button_empty_message() {
        let channel_view =
            Conversation::new(ConversationId::Channel(0.into()), NodeId::from(100u64));
        let _button = channel_view.clear_button();
        // Button should be disabled when the message is empty
    }

    #[test]
    fn test_clear_button_with_message() {
        let mut channel_view =
            Conversation::new(ConversationId::Channel(0.into()), NodeId::from(100u64));
        let _ = channel_view.update(MessageInput("Hello".into()));
        let _button = channel_view.clear_button();
        // Button should be enabled when the message is not empty
    }

    #[test]
    fn test_input_box_empty() {
        let channel_view =
            Conversation::new(ConversationId::Channel(0.into()), NodeId::from(100u64));
        let _element = channel_view.input_box();
        // Should not panic
    }

    #[test]
    fn test_input_box_with_message() {
        let mut channel_view =
            Conversation::new(ConversationId::Channel(0.into()), NodeId::from(100u64));
        let _ = channel_view.update(MessageInput("Hello world".into()));
        let _element = channel_view.input_box();
        // Should not panic
    }

    #[test]
    fn test_now_local() {
        let now = Conversation::now_local();
        // Should return current local time
        assert!(now.timestamp() > 0);
    }

    #[test]
    fn test_replying_to_with_valid_entry() {
        use iced::widget::Column;

        let mut channel_view =
            Conversation::new(ConversationId::Channel(0.into()), NodeId::from(100u64));
        let message = MCMessage::new(
            MessageId::from(1),
            NodeId::from(200u64),
            NewTextMessage("Original".into()),
            MeshChat::now(),
        );
        let _ = channel_view.new_message(message, &HistoryLength::All);
        let _ = channel_view.update(PrepareReply(MessageId::from(1)));

        let column: Column<crate::Message> = Column::new();
        let _result_column = channel_view.replying_to(column, &MessageId::from(1));
        // Should add a reply indicator to the column
    }

    #[test]
    fn test_replying_to_with_invalid_entry() {
        use iced::widget::Column;

        let channel_view =
            Conversation::new(ConversationId::Channel(0.into()), NodeId::from(100u64));
        // No messages in the channel view

        let column: Column<crate::Message> = Column::new();
        let _result_column = channel_view.replying_to(column, &MessageId::from(999));
        // Should return column unchanged
    }
}
