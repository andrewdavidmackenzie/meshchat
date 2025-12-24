use crate::styles::{
    button_chip_style, container_style, emoji_scrollbar_style, emoji_tab_style, tooltip_style,
};
use emojis::Group;
use iced::widget::Button;
use iced::widget::button::Status;
use iced::{
    Element, Length, Theme,
    widget::{button, column, container, grid, row, scrollable, text, tooltip},
};

/// Message type for EmojiPicker
#[derive(Debug, Clone)]
pub enum PickerMessage<Message> {
    GroupSelected(Group),
    EmojiSelected(Message),
}

/// State for the EmojiPicker widget
#[derive(Debug, Clone)]
pub struct EmojiPicker {
    group: Group,
    width: Length,
    height: Length,
}

impl EmojiPicker {
    pub fn new() -> Self {
        Self {
            group: Group::SmileysAndEmotion,
            width: Length::Fill,
            height: Length::Fill,
        }
    }

    /// Set the current emoji group
    pub fn with_group(mut self, group: Group) -> Self {
        self.group = group;
        self
    }

    /// Set the width of the widget
    pub fn width(mut self, width: impl Into<Length>) -> Self {
        self.width = width.into();
        self
    }

    /// Set the height of the widget
    pub fn height(mut self, height: impl Into<Length>) -> Self {
        self.height = height.into();
        self
    }

    /// Update the picker state based on messages
    /// Returns `Some(Message)` when an emoji is selected, None for group changes
    pub fn update<Message>(&mut self, message: PickerMessage<Message>) -> Option<Message> {
        match message {
            PickerMessage::GroupSelected(group) => {
                self.group = group;
                None
            }
            PickerMessage::EmojiSelected(msg) => Some(msg),
        }
    }

    /// Create the view for the emoji picker with group selection buttons
    /// The on_select closure is called with the selected emoji string and should return your Message type
    pub fn view<'a, Message>(
        &'a self,
        on_select: impl Fn(String) -> Message + 'a,
    ) -> Element<'a, PickerMessage<Message>>
    where
        Message: 'a + Clone,
    {
        const SPACING: u32 = 3;

        let groups_column = column![
            self.group_button("😀", Group::SmileysAndEmotion),
            self.group_button("👋", Group::PeopleAndBody),
            self.group_button("🐒", Group::AnimalsAndNature),
            self.group_button("🍉", Group::FoodAndDrink),
            self.group_button("🗺", Group::TravelAndPlaces),
            self.group_button("🎉", Group::Activities),
            self.group_button("📣", Group::Objects),
            self.group_button("🚮", Group::Symbols),
            self.group_button("🏁", Group::Flags),
        ]
        .spacing(SPACING);

        let emojis = self.group.emojis().collect::<Vec<_>>();
        let mut items = vec![];

        for emoji in emojis {
            items.push(Element::from(
                tooltip(
                    button(text(emoji.as_str()).center().size(30))
                        .style(button_chip_style)
                        .on_press(PickerMessage::EmojiSelected(on_select(emoji.to_string()))),
                    text(emoji.name()),
                    tooltip::Position::default(),
                )
                .style(tooltip_style),
            ));
        }

        let grid = grid(items).fluid(50).spacing(SPACING);

        container(
            row![
                groups_column,
                scrollable(container(grid).padding(6))
                    .style(emoji_scrollbar_style)
                    .spacing(SPACING)
            ]
            .spacing(-1.0),
        )
        .width(self.width)
        .height(self.height)
        .style(container_style)
        .into()
    }

    fn group_button<'a, Message>(
        &'a self,
        emoji: &'a str,
        group: Group,
    ) -> Button<'a, PickerMessage<Message>>
    where
        Message: 'a + Clone,
    {
        button(text(emoji))
            .on_press(PickerMessage::GroupSelected(group))
            .style(move |theme: &Theme, status: Status| {
                emoji_tab_style(theme, status, self.group == group)
            })
    }
}

impl Default for EmojiPicker {
    fn default() -> Self {
        Self::new()
    }
}
