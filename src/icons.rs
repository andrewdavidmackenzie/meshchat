// Generated automatically by iced_fontello at build time.
// Do not edit manually. Source: ../assets/fonts/icons.toml
// d0a59281658de0a6a8792be5cae0c4116ea839df0870d4fac7a335040354dbe2
use iced::Font;
use iced::widget::{Text, text};

pub const FONT: &[u8] = include_bytes!("../assets/fonts/icons.ttf");

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
