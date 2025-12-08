use crate::Message;
use crate::Message::{DeviceViewEvent, ShowLocation};
use crate::channel_view::{ChannelId, ChannelViewMessage};
use crate::channel_view_entry::Payload::{
    EmojiReply, NewTextMessage, PositionMessage, TextMessageReply, UserMessage,
};
use crate::device_view::DeviceViewMessage::{ChannelMsg, ShowChannel};
use crate::styles::{
    COLOR_DICTIONARY, COLOR_GREEN, MY_MESSAGE_BUBBLE_STYLE, OTHERS_MESSAGE_BUBBLE_STYLE,
    TIME_TEXT_COLOR, TIME_TEXT_SIZE, TIME_TEXT_WIDTH, button_chip_style, menu_button_style,
    message_text_style,
};
use chrono::{DateTime, Local, Utc};
use iced::Length::Fixed;
use iced::advanced::text::Shaping::Advanced;
use iced::font::Weight;
use iced::widget::{Column, Container, Row, Space, Text, button, text, tooltip};
use iced::{Bottom, Color, Element, Fill, Font, Left, Padding, Renderer, Right, Theme, Top};
use iced_aw::menu::{Item, Menu};
use iced_aw::{MenuBar, menu_bar, menu_items};
use meshtastic::protobufs::User;
use ringmap::RingMap;
use std::cmp::Ordering;
use std::collections::HashMap;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Clone, Debug)]
pub enum Payload {
    NewTextMessage(String),
    /// TextMessageReply(reply_to_id, reply text)
    TextMessageReply(u32, String),
    /// EmojiReply(reply_to_id, emoji_code string)
    EmojiReply(u32, String),
    PositionMessage(i32, i32),
    UserMessage(User),
}

/// An entry in the Channel View that represents some type of message sent to either this user on
/// this device or to a channel this device can read. Can be any of [Payload] types.
#[allow(dead_code)] // Remove when the 'seen' field is used
#[derive(Clone, Debug)]
pub struct ChannelViewEntry {
    from: u32,
    message_id: u32,
    rx_daytime: DateTime<Local>,
    payload: Payload,
    name: Option<String>,
    seen: bool,
    acked: bool,
    /// Map of emojis and for each emoji there is the string for it and a number of node ides
    /// who sent that emoji
    emoji_reply: HashMap<String, Vec<String>>,
}

impl ChannelViewEntry {
    /// Create a new [ChannelViewEntry] from the parameters provided. The received time will be set to
    /// the current time in EPOC as an u64
    pub fn new(
        payload: Payload,
        from: u32,
        message_id: u32,
        name: Option<String>,
        seen: bool,
    ) -> Self {
        let rx_time = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|t| t.as_secs())
            .unwrap_or(0);

        let datetime_utc = DateTime::<Utc>::from_timestamp_secs(rx_time as i64).unwrap();
        let rx_daytime = datetime_utc.with_timezone(&Local);

        ChannelViewEntry {
            payload,
            from,
            message_id,
            rx_daytime,
            name,
            seen,
            acked: false,
            emoji_reply: HashMap::new(),
        }
    }

    pub fn as_new_message(&self) -> Option<&String> {
        if let NewTextMessage(text_message) = self.payload() {
            Some(text_message)
        } else {
            None
        }
    }

    /// Return the node id that sent the message
    pub fn from(&self) -> u32 {
        self.from
    }

    /// Get a reference to the payload of this message
    pub fn payload(&self) -> &Payload {
        &self.payload
    }

    /// Return true if this message was sent from the specified node id
    pub fn source_node(&self, node_id: u32) -> bool {
        self.from == node_id
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
    pub fn add_emoji(&mut self, emoji_string: String, emoji_source: String) {
        self.emoji_reply
            .entry(emoji_string)
            .and_modify(|sender_vec| sender_vec.push(emoji_source.clone()))
            .or_insert(vec![emoji_source]);
    }

    /// Return true if the radio has acknowledged this message
    pub fn acked(&self) -> bool {
        self.acked
    }

    /// Return the emoji reply to this message, if any.
    pub fn emojis(&self) -> &HashMap<String, Vec<String>> {
        &self.emoji_reply
    }

    /// Return the time this message was received/sent as u64 seconds in EPOCH time
    pub fn time(&self) -> DateTime<Local> {
        self.rx_daytime
    }

    /// Return the optional name of the sender of this message
    pub fn name(&self) -> &Option<String> {
        &self.name
    }

    /// Order two messages - using the rx_daytime field.
    pub fn sort_by_rx_time(_: &u32, left: &Self, _: &u32, right: &Self) -> Ordering {
        left.rx_daytime.cmp(&right.rx_daytime)
    }

    /// Return the text of a NewTextMessage that matches the given id, or None if not found
    fn text_from_id(entries: &RingMap<u32, ChannelViewEntry>, id: u32) -> Option<&String> {
        entries.get(&id).and_then(|entry| entry.as_new_message())
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
    fn list_of_nodes(sources: &Vec<String>) -> Element<'static, Message> {
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

    /// Create an Element that contains a message received or sent
    pub fn view<'a>(
        &'a self,
        entries: &'a RingMap<u32, ChannelViewEntry>,
        mine: bool,
    ) -> Element<'a, Message> {
        let mut message_content_column = Column::new();

        // Add the source node name if there is one
        if let Some(name) = &self.name {
            let text_color = Self::color_from_name(name);
            let mut top_row = Row::new().padding(0).align_y(Top);

            if !mine {
                top_row = top_row.push(self.menu_bar()).push(Space::with_width(2.0));
            }

            top_row = top_row.push(
                text(name)
                    .shaping(Advanced)
                    .font(Font {
                        weight: Weight::Bold,
                        ..Default::default()
                    })
                    .color(text_color),
            );

            message_content_column = message_content_column.push(top_row);
        }

        let content: Element<'static, Message> = match self.payload() {
            NewTextMessage(text_msg) => text(text_msg.clone())
                .style(message_text_style)
                .size(18)
                .shaping(Advanced)
                .into(),
            TextMessageReply(reply_to_id, text_msg) => {
                if let Some(original_text) = Self::text_from_id(entries, *reply_to_id) {
                    let quote_row = Row::new()
                        .push(text("Re: ").color(COLOR_GREEN).shaping(Advanced))
                        .push(text(original_text).color(COLOR_GREEN).shaping(Advanced));
                    message_content_column = message_content_column.push(quote_row);
                };

                text(text_msg.clone())
                    .style(message_text_style)
                    .size(18)
                    .shaping(Advanced)
                    .into()
            }
            PositionMessage(lat, long) => {
                let latitude = 0.0000001 * *lat as f64;
                let longitude = 0.0000001 * *long as f64;
                button(text(format!("📌 {:.2}, {:.2}", latitude, longitude)).shaping(Advanced))
                    .padding([1, 5])
                    .style(button_chip_style)
                    .on_press(ShowLocation(latitude, longitude))
                    .into()
            }
            UserMessage(user) => text(format!(
                "ⓘ from {} ({}), id = '{}', with hardware '{}'",
                user.long_name,
                user.short_name,
                user.id,
                user.hw_model().as_str_name()
            ))
            .style(message_text_style)
            .size(18)
            .shaping(Advanced)
            .into(),
            EmojiReply(_, _) => text("").into(), // Should never happen
        };

        // Create the row with message text and time and maybe an ACK tick mark
        let mut text_and_time_row = Row::new()
            .push(content)
            .push(Space::with_width(10.0))
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
        if !self.emojis().is_empty() {
            let mut emoji_row = Row::new().padding(Padding {
                top: -4.0,
                right: 6.0,
                bottom: 6.0,
                left: 6.0,
            });
            for (emoji, sources) in self.emojis() {
                let tooltip_element: Element<'_, Message> = Self::list_of_nodes(sources);
                emoji_row = emoji_row.push(
                    tooltip(
                        text(emoji.clone()).size(18).align_y(Top).shaping(Advanced),
                        tooltip_element,
                        tooltip::Position::Bottom,
                    )
                    .padding(0),
                );
            }
            message_column = message_column.push(emoji_row);
        }

        message_column.into()
    }

    // TODO differentiate if we are in a node or channel view
    // if a node view, don't show the DM menu item
    fn menu_bar<'a>(&self) -> MenuBar<'a, Message, Theme, Renderer> {
        let menu_tpl_1 = |items| Menu::new(items).spacing(3);

        let dm = format!("DM with {}", self.name.as_ref().unwrap_or(&"".to_string()));
        #[rustfmt::skip]
        let menu_items = menu_items!(
            //(menu_button("forward".into(), Message::None))
            //(menu_button("copy".into(), Message::None))
            (menu_button("reply".into(), DeviceViewEvent(ChannelMsg(ChannelViewMessage::PrepareReply(self.message_id)))))
            //(menu_button("react".into(), Message::None))
            (menu_button(dm, DeviceViewEvent(ShowChannel(Some(ChannelId::Node(self.from()))))))
        );

        // Create the menu bar with the root button and list of options
        menu_bar!((menu_root_button("▼"), {
            menu_tpl_1(menu_items).width(140)
        }))
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
