use iced::border::Radius;
use iced::widget::container::Style;
use iced::widget::{button, text, text_input};
use iced::{Background, Border, Color, Shadow, Theme};

pub const TEXT_INPUT_BACKGROUND: Background =
    Background::Color(Color::from_rgba(0.25, 0.25, 0.25, 1.0));

const TEXT_INPUT_R: f32 = 20.0;

const RADIUS_2: Radius = Radius {
    top_left: 2.0,
    top_right: 2.0,
    bottom_right: 2.0,
    bottom_left: 2.0,
};

pub const NO_SHADOW: Shadow = Shadow {
    color: Color::TRANSPARENT,
    offset: iced::Vector { x: 0.0, y: 0.0 },
    blur_radius: 0.0,
};

pub const NO_BORDER: Border = Border {
    color: Color::TRANSPARENT,
    width: 0.0,
    radius: RADIUS_2,
};

pub const RADIUS_12: Radius = Radius {
    top_left: 12.0,
    top_right: 12.0,
    bottom_right: 12.0,
    bottom_left: 12.0,
};

pub const VIEW_BUTTON_BORDER: Border = Border {
    color: Color::TRANSPARENT,
    width: 2.0,
    radius: TEXT_INPUT_RADIUS,
};

const TEXT_INPUT_RADIUS: Radius = Radius {
    top_left: TEXT_INPUT_R,
    top_right: TEXT_INPUT_R,
    bottom_right: TEXT_INPUT_R,
    bottom_left: TEXT_INPUT_R,
};

pub const CYAN: Color = Color::from_rgba(0.0, 0.8, 0.8, 1.0);

const TEXT_INPUT_BORDER_ACTIVE: Border = Border {
    radius: TEXT_INPUT_RADIUS, // rounded corners
    width: 2.0,
    color: CYAN,
};

const TEXT_INPUT_BORDER: Border = Border {
    radius: TEXT_INPUT_RADIUS, // rounded corners
    width: 2.0,
    color: Color::WHITE,
};

const TEXT_INPUT_BORDER_DISABLED: Border = Border {
    radius: TEXT_INPUT_RADIUS, // rounded corners
    width: 2.0,
    color: Color::TRANSPARENT,
};

pub const TEXT_INPUT_PLACEHOLDER_COLOR: Color = Color::from_rgba(0.5, 0.5, 0.5, 1.0);

pub fn text_input_style(_theme: &Theme, status: text_input::Status) -> text_input::Style {
    match status {
        text_input::Status::Active => text_input::Style {
            background: TEXT_INPUT_BACKGROUND,
            border: TEXT_INPUT_BORDER,
            icon: Color::WHITE,
            placeholder: TEXT_INPUT_PLACEHOLDER_COLOR,
            value: Color::WHITE,
            selection: Default::default(),
        },
        text_input::Status::Hovered => text_input::Style {
            background: TEXT_INPUT_BACKGROUND,
            border: TEXT_INPUT_BORDER,
            icon: Color::WHITE,
            placeholder: TEXT_INPUT_PLACEHOLDER_COLOR,
            value: Color::WHITE,
            selection: Default::default(),
        },
        text_input::Status::Focused => text_input::Style {
            background: TEXT_INPUT_BACKGROUND,
            border: TEXT_INPUT_BORDER_ACTIVE,
            icon: Color::WHITE,
            placeholder: TEXT_INPUT_PLACEHOLDER_COLOR,
            value: Color::WHITE,
            selection: Default::default(),
        },
        text_input::Status::Disabled => text_input::Style {
            background: TEXT_INPUT_BACKGROUND,
            border: TEXT_INPUT_BORDER_DISABLED,
            icon: Color::WHITE,
            placeholder: TEXT_INPUT_PLACEHOLDER_COLOR,
            value: Color::WHITE,
            selection: Default::default(),
        },
    }
}

const BUTTON_BORDER_ACTIVE: Border = Border {
    radius: TEXT_INPUT_RADIUS, // rounded corners
    width: 2.0,
    color: CYAN,
};

pub fn chip_style(_theme: &Theme, status: button::Status) -> button::Style {
    match status {
        button::Status::Active => button::Style {
            background: Some(Background::Color(Color::from_rgba(0.0, 0.0, 0.8, 1.0))),
            text_color: Color::WHITE,
            border: BUTTON_BORDER_ACTIVE,
            shadow: NO_SHADOW,
        },
        button::Status::Hovered => button::Style {
            background: Some(Background::Color(CYAN)),
            text_color: Color::WHITE,
            border: BUTTON_BORDER_ACTIVE,
            shadow: NO_SHADOW,
        },
        button::Status::Pressed => button::Style {
            background: Some(Background::Color(Color::from_rgba(0.0, 0.0, 0.8, 1.0))),
            text_color: Color::WHITE,
            border: BUTTON_BORDER_ACTIVE,
            shadow: NO_SHADOW,
        },
        button::Status::Disabled => button::Style {
            background: Some(Background::Color(Color::from_rgba(0.0, 0.0, 0.8, 1.0))),
            text_color: Color::WHITE,
            border: BUTTON_BORDER_ACTIVE,
            shadow: NO_SHADOW,
        },
    }
}

const MESSAGE_BORDER: Border = Border {
    radius: RADIUS_12, // rounded corners
    width: 2.0,
    color: Color::WHITE,
};

const DAY_SEPARATOR_BORDER: Border = Border {
    radius: RADIUS_12, // rounded corners
    width: 2.0,
    color: Color::TRANSPARENT,
};

// TODO make a closure that reacts to the theme
pub const MESSAGE_TEXT_STYLE: text::Style = text::Style {
    color: Some(Color::WHITE),
};

pub const TIME_TEXT_COLOR: Color = Color::from_rgba(0.6, 0.6, 0.6, 1.0);
pub const TIME_TEXT_SIZE: f32 = 11.0;
pub const TIME_TEXT_WIDTH: f32 = 60.0;

pub const MY_MESSAGE_BUBBLE_STYLE: Style = Style {
    text_color: Some(Color::WHITE),
    background: Some(Background::Color(Color::from_rgba(0.08, 0.3, 0.22, 1.0))),
    border: MESSAGE_BORDER,
    shadow: NO_SHADOW,
};

pub const DAY_SEPARATOR_STYLE: Style = Style {
    text_color: Some(Color::WHITE),
    background: Some(Background::Color(Color::from_rgba(0.0, 0.0, 0.0, 1.0))),
    border: DAY_SEPARATOR_BORDER,
    shadow: NO_SHADOW,
};

pub const OTHERS_MESSAGE_BUBBLE_STYLE: Style = Style {
    text_color: Some(Color::WHITE),
    background: Some(Background::Color(Color::from_rgba(0.0, 0.0, 0.0, 1.0))),
    border: MESSAGE_BORDER,
    shadow: NO_SHADOW,
};
