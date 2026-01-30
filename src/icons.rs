// Generated automatically by iced_fontello at build time.
// Do not edit manually. Source: ../assets/fonts/icons.toml
// d2cb544f64bb87bdce4c06cd7f6d60b65c250358694d0597a944012c01431e91
use iced::widget::{text, Text};
use iced::Font;

pub const FONT: &[u8] = include_bytes!("../assets/fonts/icons.ttf");

pub fn cog<'a>() -> Text<'a> {
    icon("\u{2699}")
}

pub fn send<'a>() -> Text<'a> {
    icon("\u{2709}")
}

pub fn share<'a>() -> Text<'a> {
    icon("\u{F1E0}")
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
