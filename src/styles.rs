#![allow(dead_code)] // for extra colors we have generated but not used yet

use crate::battery::Appearance as BatteryAppearance;
use iced::border::Radius;
use iced::widget::button::Status;
use iced::widget::button::Status::Hovered;
use iced::widget::container::Style;
use iced::widget::scrollable::{AutoScroll, Rail, Scroller};
use iced::widget::{button, scrollable, text, text_input};
use iced::{Background, Border, Color, Shadow, Theme};
use iced_aw::menu;
use iced_aw::style::colors::RED;

// Basic Colors
pub const COLOR_RED: Color = Color::from_rgb(0.9, 0.2, 0.2);
pub const COLOR_GREEN: Color = Color::from_rgb(0.2, 0.8, 0.2);
pub const COLOR_BLUE: Color = Color::from_rgb(0.2, 0.4, 0.9);
pub const COLOR_YELLOW: Color = Color::from_rgb(0.95, 0.9, 0.2);
pub const COLOR_ORANGE: Color = Color::from_rgb(0.95, 0.6, 0.2);
pub const COLOR_PURPLE: Color = Color::from_rgb(0.7, 0.3, 0.9);
pub const COLOR_PINK: Color = Color::from_rgb(0.95, 0.5, 0.7);
pub const COLOR_CYAN: Color = Color::from_rgb(0.2, 0.8, 0.9);

// Pastel Colors
pub const COLOR_PASTEL_BLUE: Color = Color::from_rgb(0.68, 0.85, 0.9);
pub const COLOR_PASTEL_GREEN: Color = Color::from_rgb(0.7, 0.93, 0.77);
pub const COLOR_PASTEL_YELLOW: Color = Color::from_rgb(0.99, 0.96, 0.7);
pub const COLOR_PASTEL_PINK: Color = Color::from_rgb(0.98, 0.8, 0.85);
pub const COLOR_PASTEL_PURPLE: Color = Color::from_rgb(0.8, 0.7, 0.9);
pub const COLOR_PASTEL_ORANGE: Color = Color::from_rgb(0.98, 0.85, 0.7);

// Dark Colors
pub const COLOR_DARK_RED: Color = Color::from_rgb(0.5, 0.1, 0.1);
pub const COLOR_DARK_GREEN: Color = Color::from_rgb(0.1, 0.4, 0.1);
pub const COLOR_DARK_BLUE: Color = Color::from_rgb(0.1, 0.2, 0.5);
pub const COLOR_DARK_GRAY: Color = Color::from_rgb(0.2, 0.2, 0.2);
pub const COLOR_DARK_PURPLE: Color = Color::from_rgb(0.3, 0.1, 0.4);

// Gray Scale
pub const COLOR_GRAY_10: Color = Color::from_rgb(0.1, 0.1, 0.1);
pub const COLOR_GRAY_20: Color = Color::from_rgb(0.2, 0.2, 0.2);
pub const COLOR_GRAY_30: Color = Color::from_rgb(0.3, 0.3, 0.3);
pub const COLOR_GRAY_40: Color = Color::from_rgb(0.4, 0.4, 0.4);
pub const COLOR_GRAY_50: Color = Color::from_rgb(0.5, 0.5, 0.5);
pub const COLOR_GRAY_60: Color = Color::from_rgb(0.6, 0.6, 0.6);
pub const COLOR_GRAY_70: Color = Color::from_rgb(0.7, 0.7, 0.7);
pub const COLOR_GRAY_80: Color = Color::from_rgb(0.8, 0.8, 0.8);
pub const COLOR_GRAY_90: Color = Color::from_rgb(0.9, 0.9, 0.9);

// Vibrant Colors
pub const COLOR_VIBRANT_RED: Color = Color::from_rgb(1.0, 0.0, 0.2);
pub const COLOR_VIBRANT_GREEN: Color = Color::from_rgb(0.0, 1.0, 0.3);
pub const COLOR_VIBRANT_BLUE: Color = Color::from_rgb(0.0, 0.5, 1.0);
pub const COLOR_VIBRANT_MAGENTA: Color = Color::from_rgb(1.0, 0.0, 1.0);
pub const COLOR_VIBRANT_CYAN: Color = Color::from_rgb(0.0, 1.0, 1.0);
pub const COLOR_VIBRANT_YELLOW: Color = Color::from_rgb(1.0, 1.0, 0.0);

// A dictionary of colors to use for names of other nodes
pub const COLOR_DICTIONARY: [Color; 15] = [
    COLOR_RED,
    COLOR_GREEN,
    COLOR_BLUE,
    COLOR_CYAN,
    COLOR_PINK,
    COLOR_YELLOW,
    COLOR_GREEN,
    COLOR_ORANGE,
    COLOR_PURPLE,
    COLOR_VIBRANT_BLUE,
    COLOR_VIBRANT_MAGENTA,
    COLOR_PASTEL_BLUE,
    COLOR_PASTEL_GREEN,
    COLOR_PASTEL_PINK,
    COLOR_PASTEL_PURPLE,
];

pub const TEXT_INPUT_BACKGROUND: Background =
    Background::Color(Color::from_rgba(0.25, 0.25, 0.25, 1.0));

const TEXT_INPUT_R: f32 = 20.0;

const RADIUS_2: Radius = Radius {
    top_left: 2.0,
    top_right: 2.0,
    bottom_right: 2.0,
    bottom_left: 2.0,
};

const RADIUS_4: Radius = Radius {
    top_left: 4.0,
    top_right: 4.0,
    bottom_right: 4.0,
    bottom_left: 4.0,
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

pub const CYAN_BORDER: Border = Border {
    color: CYAN,
    width: 2.0,
    radius: RADIUS_2,
};

pub const RADIUS_12: Radius = Radius {
    top_left: 12.0,
    top_right: 12.0,
    bottom_right: 12.0,
    bottom_left: 12.0,
};

pub const TOOLTIP_BORDER: Border = Border {
    color: Color::WHITE,
    width: 1.0,
    radius: RADIUS_12,
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
        text_input::Status::Focused { .. } => text_input::Style {
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

const BUTTON_BORDER_DISABLED: Border = Border {
    radius: TEXT_INPUT_RADIUS, // rounded corners
    width: 2.0,
    color: Color::TRANSPARENT,
};

pub fn source_tooltip_style(_theme: &Theme) -> Style {
    Style {
        text_color: Some(Color::WHITE),
        background: Some(Color::BLACK.into()),
        border: TOOLTIP_BORDER,
        shadow: NO_SHADOW,
        snap: false,
    }
}

pub fn transparent_button_style(_theme: &Theme, status: Status, color: Color) -> button::Style {
    let mut style = match status {
        Status::Active => button::Style {
            background: None,
            text_color: Color::WHITE,
            border: NO_BORDER,
            shadow: NO_SHADOW,
            snap: false,
        },
        Hovered => button::Style {
            background: None,
            text_color: Color::WHITE,
            border: NO_BORDER,
            shadow: NO_SHADOW,
            snap: false,
        },
        Status::Pressed => button::Style {
            background: None,
            text_color: Color::WHITE,
            border: NO_BORDER,
            shadow: NO_SHADOW,
            snap: false,
        },
        Status::Disabled => button::Style {
            background: None,
            text_color: Color::WHITE,
            border: NO_BORDER,
            shadow: NO_SHADOW,
            snap: false,
        },
    };

    style.text_color = color;
    style
}

pub fn fav_button_style(_theme: &Theme, status: Status) -> button::Style {
    match status {
        Status::Active => button::Style {
            background: Some(Background::Color(Color::TRANSPARENT)),
            text_color: Color::WHITE,
            border: NO_BORDER,
            shadow: NO_SHADOW,
            snap: false,
        },
        Hovered => button::Style {
            background: Some(Background::Color(CYAN)),
            text_color: Color::WHITE,
            border: BUTTON_BORDER_ACTIVE,
            shadow: NO_SHADOW,
            snap: false,
        },
        Status::Pressed => button::Style {
            background: Some(Background::Color(Color::from_rgba(0.0, 0.0, 0.8, 1.0))),
            text_color: Color::WHITE,
            border: BUTTON_BORDER_ACTIVE,
            shadow: NO_SHADOW,
            snap: false,
        },
        Status::Disabled => button::Style {
            background: Some(Background::Color(Color::TRANSPARENT)),
            text_color: Color::WHITE,
            border: BUTTON_BORDER_DISABLED,
            shadow: NO_SHADOW,
            snap: false,
        },
    }
}

pub fn button_chip_style(_theme: &Theme, status: Status) -> button::Style {
    match status {
        Status::Active => button::Style {
            background: Some(Background::Color(Color::from_rgba(0.0, 0.0, 0.8, 1.0))),
            text_color: Color::WHITE,
            border: BUTTON_BORDER_ACTIVE,
            shadow: NO_SHADOW,
            snap: false,
        },
        Hovered => button::Style {
            background: Some(Background::Color(CYAN)),
            text_color: Color::WHITE,
            border: BUTTON_BORDER_ACTIVE,
            shadow: NO_SHADOW,
            snap: false,
        },
        Status::Pressed => button::Style {
            background: Some(Background::Color(Color::from_rgba(0.0, 0.0, 0.8, 1.0))),
            text_color: Color::WHITE,
            border: BUTTON_BORDER_ACTIVE,
            shadow: NO_SHADOW,
            snap: false,
        },
        Status::Disabled => button::Style {
            background: Some(Background::Color(Color::from_rgba(0.0, 0.0, 0.8, 1.0))),
            text_color: Color::WHITE,
            border: BUTTON_BORDER_DISABLED,
            shadow: NO_SHADOW,
            snap: false,
        },
    }
}

const MESSAGE_BORDER: Border = Border {
    radius: RADIUS_12, // rounded corners
    width: 2.0,
    color: COLOR_GRAY_20,
};

const DAY_SEPARATOR_BORDER: Border = Border {
    radius: RADIUS_12, // rounded corners
    width: 2.0,
    color: Color::TRANSPARENT,
};

pub fn message_text_style(_theme: &Theme) -> text::Style {
    text::Style {
        color: Some(Color::WHITE),
    }
}

pub fn alert_message_style(_theme: &Theme) -> text::Style {
    text::Style { color: Some(RED) }
}

pub const TIME_TEXT_COLOR: Color = Color::from_rgba(0.6, 0.6, 0.6, 1.0);
pub const TIME_TEXT_SIZE: f32 = 11.0;
pub const TIME_TEXT_WIDTH: f32 = 30.0;

pub const MY_MESSAGE_BUBBLE_STYLE: Style = Style {
    text_color: Some(Color::WHITE),
    background: Some(Background::Color(Color::from_rgba(0.08, 0.3, 0.22, 1.0))),
    border: MESSAGE_BORDER,
    shadow: NO_SHADOW,
    snap: false,
};

pub const DAY_SEPARATOR_STYLE: Style = Style {
    text_color: Some(Color::WHITE),
    background: Some(Background::Color(Color::from_rgba(0.0, 0.0, 0.0, 1.0))),
    border: DAY_SEPARATOR_BORDER,
    shadow: NO_SHADOW,
    snap: false,
};

pub const OTHERS_MESSAGE_BUBBLE_STYLE: Style = Style {
    text_color: Some(Color::WHITE),
    background: Some(Background::Color(Color::from_rgba(0.0, 0.0, 0.0, 1.0))),
    border: MESSAGE_BORDER,
    shadow: NO_SHADOW,
    snap: false,
};

const NAME_BOX_BORDER: Border = Border {
    radius: Radius {
        top_left: 6.0,
        top_right: 6.0,
        bottom_right: 6.0,
        bottom_left: 6.0,
    }, // rounded corners
    width: 1.0,
    color: Color::WHITE,
};

pub const NAME_BOX_STYLE: Style = Style {
    text_color: Some(Color::WHITE),
    background: Some(Background::Color(Color::from_rgba(0.0, 0.0, 0.0, 1.0))),
    border: NO_BORDER,
    shadow: NO_SHADOW,
    snap: false,
};

pub fn name_box_style(_theme: &Theme) -> Style {
    NAME_BOX_STYLE
}

const REPLY_TO_STYLE: Style = Style {
    text_color: Some(Color::WHITE),
    background: Some(Background::Color(Color::from_rgba(0.08, 0.3, 0.22, 1.0))),
    border: NO_BORDER,
    shadow: NO_SHADOW,
    snap: false,
};

pub fn reply_to_style(_theme: &Theme) -> Style {
    REPLY_TO_STYLE
}

pub const VIEW_BUTTON_HOVER_STYLE: button::Style = button::Style {
    background: Some(Background::Color(Color::from_rgba(0.0, 0.8, 0.8, 1.0))),
    text_color: Color::BLACK,
    border: VIEW_BUTTON_BORDER,
    shadow: NO_SHADOW,
    snap: false,
};

pub const VIEW_BUTTON_STYLE: button::Style = button::Style {
    background: Some(Background::Color(Color::from_rgba(0.0, 1.0, 1.0, 0.0))),
    text_color: Color::WHITE,
    border: NO_BORDER,
    shadow: NO_SHADOW,
    snap: false,
};

pub fn channel_row_style(_: &Theme, status: Status) -> button::Style {
    if status == Hovered {
        VIEW_BUTTON_HOVER_STYLE
    } else {
        VIEW_BUTTON_STYLE
    }
}

pub fn error_notification_style(_theme: &Theme) -> Style {
    Style {
        text_color: Some(Color::WHITE),
        background: Some(Background::Color(COLOR_RED)),
        border: Border {
            radius: Radius::from(12.0), // rounded corners
            width: 2.0,
            color: COLOR_YELLOW,
        },
        ..Default::default()
    }
}

pub fn info_notification_style(_theme: &Theme) -> Style {
    Style {
        text_color: Some(Color::WHITE),
        background: Some(Background::Color(Color::from_rgb8(0x00, 0x00, 0x00))), // black
        border: Border {
            radius: Radius::from(12.0), // rounded corners
            width: 2.0,
            color: Color::WHITE,
        },
        ..Default::default()
    }
}

pub fn menu_button_style(_theme: &Theme, _status: iced_aw::style::Status) -> menu::Style {
    menu::Style {
        bar_background: Background::Color(Color::TRANSPARENT),
        bar_border: NO_BORDER,
        bar_shadow: NO_SHADOW,
        menu_background: Background::Color(Color::BLACK),
        menu_border: BUTTON_BORDER_DISABLED,
        menu_shadow: NO_SHADOW,
        path: Background::Color(Color::TRANSPARENT),
        path_border: NO_BORDER,
    }
}

/// Battery widget style using dark theme colors
/// Returns the dark style appearance for the battery widget
pub fn battery_style(_theme: &Theme) -> BatteryAppearance {
    BatteryAppearance {
        background_color: COLOR_GRAY_10,   // Very dark background
        border_color: COLOR_GRAY_80,       // Light gray border
        charging_color: COLOR_GREEN,       // Green for charging
        charge_high_color: COLOR_GREEN,    // Dark green for high charge (>50%)
        charge_medium_color: COLOR_ORANGE, // Orange for medium charge (20-50%)
        charge_low_color: COLOR_RED,       // Dark red for low charge (<20%)
        unknown_color: COLOR_GRAY_40,      // Medium gray for unknown state
    }
}

/// Dark variant battery style
pub fn battery_style_dark(_theme: &Theme) -> BatteryAppearance {
    BatteryAppearance {
        background_color: COLOR_GRAY_10,
        border_color: COLOR_GRAY_80,
        charging_color: COLOR_GREEN,
        charge_high_color: COLOR_GREEN,
        charge_medium_color: COLOR_ORANGE,
        charge_low_color: COLOR_RED,
        unknown_color: COLOR_GRAY_40,
    }
}

pub fn scrollbar_style(_theme: &Theme, status: scrollable::Status) -> scrollable::Style {
    let scrollbar_color = match status {
        scrollable::Status::Active { .. } => Background::Color(Color::TRANSPARENT),
        scrollable::Status::Hovered { .. } => Background::Color(COLOR_GRAY_80),
        scrollable::Status::Dragged { .. } => Background::Color(CYAN),
    };

    let border = match status {
        scrollable::Status::Active { .. } => NO_BORDER,
        scrollable::Status::Hovered { .. } => CYAN_BORDER,
        scrollable::Status::Dragged { .. } => NO_BORDER,
    };

    scrollable::Style {
        container: Style {
            text_color: None,
            background: Some(Background::Color(Color::TRANSPARENT)),
            border: NO_BORDER,
            shadow: NO_SHADOW,
            snap: false,
        },
        vertical_rail: Rail {
            background: Some(Background::Color(Color::TRANSPARENT)),
            border: NO_BORDER,
            scroller: Scroller {
                background: scrollbar_color,
                border,
            },
        },
        horizontal_rail: Rail {
            background: Some(Background::Color(Color::TRANSPARENT)),
            border: NO_BORDER,
            scroller: Scroller {
                background: scrollbar_color,
                border,
            },
        },
        gap: None,
        auto_scroll: AutoScroll {
            background: Background::Color(Color::TRANSPARENT),
            border,
            shadow: NO_SHADOW,
            icon: Default::default(),
        },
    }
}
