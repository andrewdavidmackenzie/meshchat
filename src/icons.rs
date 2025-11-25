// Generated automatically by iced_fontello at build time.
// Do not edit manually. Source: ../assets/fonts/icons.toml
// 555179be61bd876469fd674a261017127e7946db7d41f14a582b361a3e23c646
use iced::Font;
use iced::widget::{Text, text};

pub const FONT: &[u8] = include_bytes!("../assets/fonts/icons.ttf");

pub fn send<'a>() -> Text<'a> {
    icon("\u{2709}")
}

pub fn star<'a>() -> Text<'a> {
    icon("\u{2605}")
}

pub fn star_empty<'a>() -> Text<'a> {
    icon("\u{2606}")
}

fn icon(codepoint: &str) -> Text<'_> {
    text(codepoint).font(Font::with_name("icons"))
}
