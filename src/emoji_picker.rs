use emojis::Group;
use iced::{
    Element, Length, Theme,
    widget::{
        button, column, container, grid, row, scrollable, text, text::Shaping::Advanced, tooltip,
    },
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
    /// Returns Some(Message) when an emoji is selected, None for group changes
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
    pub fn view<'a, Message: 'a>(
        &self,
        on_select: impl Fn(String) -> Message + 'a,
    ) -> Element<'a, PickerMessage<Message>>
    where
        Message: Clone,
    {
        const SPACING: u32 = 3;

        let groups = column![
            button(text("😀")).on_press(PickerMessage::GroupSelected(Group::SmileysAndEmotion)),
            button(text("👋")).on_press(PickerMessage::GroupSelected(Group::PeopleAndBody)),
            button(text("🐒")).on_press(PickerMessage::GroupSelected(Group::AnimalsAndNature)),
            button(text("🍉")).on_press(PickerMessage::GroupSelected(Group::FoodAndDrink)),
            button(text("🗺️").shaping(Advanced))
                .on_press(PickerMessage::GroupSelected(Group::TravelAndPlaces)),
            button(text("🎉")).on_press(PickerMessage::GroupSelected(Group::Activities)),
            button(text("📣")).on_press(PickerMessage::GroupSelected(Group::Objects)),
            button(text("🚮")).on_press(PickerMessage::GroupSelected(Group::Symbols)),
            button(text("🏁")).on_press(PickerMessage::GroupSelected(Group::Flags)),
        ]
        .spacing(SPACING);

        let emojis = self.group.emojis().collect::<Vec<_>>();
        let mut items = vec![];

        for emoji in emojis {
            items.push(Element::from(
                tooltip(
                    button(text(emoji.as_str()).center().shaping(Advanced).size(30))
                        .on_press(PickerMessage::EmojiSelected(on_select(emoji.to_string()))),
                    text(emoji.name()),
                    tooltip::Position::default(),
                )
                .style(|theme: &Theme| container::Style {
                    background: Some(theme.palette().background.into()),
                    ..Default::default()
                }),
            ));
        }

        let grid = grid(items).fluid(50).spacing(SPACING);

        container(row![groups, scrollable(grid).spacing(SPACING)].spacing(10))
            .width(self.width)
            .height(self.height)
            .into()
    }
}

impl Default for EmojiPicker {
    fn default() -> Self {
        Self::new()
    }
}
