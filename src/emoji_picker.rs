use emojis::{Emoji, Group};
use iced::{
    Element,
    widget::{
        button, column, container, grid, row, scrollable, text, text::Shaping::Advanced, tooltip,
    },
};

#[derive(Debug)]
pub struct EmojiPicker {
    group: Group,
}

#[derive(Debug, Clone)]
pub enum Message {
    Group(Group),
}

impl EmojiPicker {
    pub fn new() -> Self {
        Self {
            ..Default::default()
        }
    }

    pub fn update(&mut self, message: Message) {
        match message {
            Message::Group(group) => {
                self.group = group;
            }
        }
    }
    pub fn view<'a, Message>(&self, on_press: fn(&'static Emoji) -> Message) -> Element<'a, Message>
    where
        self::Message: Into<Message>,
        Message: 'a + Clone,
    {
        const SPACING: u32 = 3;

        let groups = column![
            button(text("😀")).on_press(self::Message::Group(Group::SmileysAndEmotion).into()),
            button(text("👋")).on_press(self::Message::Group(Group::PeopleAndBody).into()),
            button(text("🐒")).on_press(self::Message::Group(Group::AnimalsAndNature).into()),
            button(text("🍉")).on_press(self::Message::Group(Group::FoodAndDrink).into()),
            button(text("🗺️").shaping(Advanced))
                .on_press(self::Message::Group(Group::TravelAndPlaces).into()),
            button(text("🎉")).on_press(self::Message::Group(Group::Activities).into()),
            button(text("📣")).on_press(self::Message::Group(Group::Objects).into()),
            button(text("🚮")).on_press(self::Message::Group(Group::Symbols).into()),
            button(text("🏁")).on_press(self::Message::Group(Group::Flags).into()),
        ]
        .spacing(SPACING);

        let emojis = self.group.emojis().collect::<Vec<_>>();
        let mut items = vec![];

        for emoji in emojis {
            items.push(Element::from(
                tooltip(
                    button(text(emoji.as_str()).center().shaping(Advanced).size(30))
                        .on_press(on_press(emoji)),
                    text(emoji.name()),
                    tooltip::Position::default(),
                )
                .style(|style| container::Style {
                    background: Some(style.palette().background.into()),
                    ..Default::default()
                }),
            ));
        }

        let grid = grid(items).fluid(50).spacing(SPACING);

        row![groups, scrollable(grid).spacing(SPACING)]
            .spacing(10)
            .into()
    }
}

impl Default for EmojiPicker {
    fn default() -> Self {
        Self {
            group: Group::SmileysAndEmotion,
        }
    }
}
