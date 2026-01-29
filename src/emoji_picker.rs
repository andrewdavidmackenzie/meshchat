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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new() {
        let picker = EmojiPicker::new();
        assert_eq!(picker.group, Group::SmileysAndEmotion);
        assert_eq!(picker.width, Length::Fill);
        assert_eq!(picker.height, Length::Fill);
    }

    #[test]
    fn test_default() {
        let picker = EmojiPicker::default();
        assert_eq!(picker.group, Group::SmileysAndEmotion);
    }

    #[test]
    fn test_with_group() {
        let picker = EmojiPicker::new().with_group(Group::FoodAndDrink);
        assert_eq!(picker.group, Group::FoodAndDrink);
    }

    #[test]
    fn test_width() {
        let picker = EmojiPicker::new().width(Length::Fixed(300.0));
        assert_eq!(picker.width, Length::Fixed(300.0));
    }

    #[test]
    fn test_height() {
        let picker = EmojiPicker::new().height(Length::Fixed(200.0));
        assert_eq!(picker.height, Length::Fixed(200.0));
    }

    #[test]
    fn test_chained_builders() {
        let picker = EmojiPicker::new()
            .with_group(Group::AnimalsAndNature)
            .width(Length::Fixed(400.0))
            .height(Length::Fixed(300.0));

        assert_eq!(picker.group, Group::AnimalsAndNature);
        assert_eq!(picker.width, Length::Fixed(400.0));
        assert_eq!(picker.height, Length::Fixed(300.0));
    }

    #[test]
    fn test_update_group_selected() {
        let mut picker = EmojiPicker::new();
        assert_eq!(picker.group, Group::SmileysAndEmotion);

        let result: Option<String> = picker.update(PickerMessage::GroupSelected(Group::Flags));

        assert!(result.is_none()); // Group changes don't return a message
        assert_eq!(picker.group, Group::Flags);
    }

    #[test]
    fn test_update_emoji_selected() {
        let mut picker = EmojiPicker::new();

        let result: Option<String> =
            picker.update(PickerMessage::EmojiSelected("selected_emoji".to_string()));

        assert_eq!(result, Some("selected_emoji".to_string()));
        // Group should remain unchanged
        assert_eq!(picker.group, Group::SmileysAndEmotion);
    }

    #[test]
    fn test_all_groups() {
        let mut picker = EmojiPicker::new();

        // Test all emoji groups can be selected
        let groups = [
            Group::SmileysAndEmotion,
            Group::PeopleAndBody,
            Group::AnimalsAndNature,
            Group::FoodAndDrink,
            Group::TravelAndPlaces,
            Group::Activities,
            Group::Objects,
            Group::Symbols,
            Group::Flags,
        ];

        for group in groups {
            let _: Option<String> = picker.update(PickerMessage::GroupSelected(group));
            assert_eq!(picker.group, group);
        }
    }

    #[test]
    fn test_group_has_emojis() {
        // Verify each group has at least some emojis
        let groups = [
            Group::SmileysAndEmotion,
            Group::PeopleAndBody,
            Group::AnimalsAndNature,
            Group::FoodAndDrink,
            Group::TravelAndPlaces,
            Group::Activities,
            Group::Objects,
            Group::Symbols,
            Group::Flags,
        ];

        for group in groups {
            let count = group.emojis().count();
            assert!(count > 0, "Group {:?} should have emojis", group);
        }
    }

    #[test]
    fn test_picker_message_clone() {
        let msg: PickerMessage<String> = PickerMessage::GroupSelected(Group::FoodAndDrink);
        let cloned = msg.clone();
        match cloned {
            PickerMessage::GroupSelected(g) => assert_eq!(g, Group::FoodAndDrink),
            _ => panic!("Expected GroupSelected"),
        }
    }

    #[test]
    fn test_picker_message_debug() {
        let msg: PickerMessage<String> = PickerMessage::GroupSelected(Group::Symbols);
        let debug = format!("{:?}", msg);
        assert!(debug.contains("GroupSelected"));
    }

    #[test]
    fn test_emoji_selected_with_custom_type() {
        #[derive(Debug, Clone, PartialEq)]
        enum MyMessage {
            EmojiChosen(String),
        }

        let mut picker = EmojiPicker::new();

        let result: Option<MyMessage> = picker.update(PickerMessage::EmojiSelected(
            MyMessage::EmojiChosen("👍".to_string()),
        ));

        assert_eq!(result, Some(MyMessage::EmojiChosen("👍".to_string())));
    }
}
