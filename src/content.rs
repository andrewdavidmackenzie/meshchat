use crate::Message;
use crate::Message::OpenUrl;
use crate::styles::button_chip_style;
use iced::Theme;
use iced::widget::text::Style;
use iced::widget::{Row, text};
use meshtastic::protobufs::User;
use std::fmt;
use std::fmt::Formatter;
use url::Url;

#[derive(Clone, Debug)]
pub enum ContentChunk {
    Text(String),
    Link(String, String),
}

#[derive(Clone, Debug, Default)]
pub struct Content {
    inner: Vec<ContentChunk>,
}

impl From<String> for ContentChunk {
    fn from(value: String) -> Self {
        ContentChunk::Text(value)
    }
}

impl From<&str> for ContentChunk {
    fn from(value: &str) -> Self {
        ContentChunk::Text(value.into())
    }
}

impl From<&Url> for ContentChunk {
    fn from(url: &Url) -> Self {
        ContentChunk::Link(url.to_string(), url.to_string())
    }
}

impl From<&Url> for Content {
    fn from(url: &Url) -> Self {
        Self {
            inner: vec![url.into()],
        }
    }
}

impl From<String> for Content {
    fn from(value: String) -> Self {
        Self {
            inner: vec![value.into()],
        }
    }
}

impl From<&str> for Content {
    fn from(value: &str) -> Self {
        Self {
            inner: vec![value.into()],
        }
    }
}

impl From<&User> for Content {
    fn from(user: &User) -> Self {
        format!(
            "ⓘ from '{}' ('{}'), id = '{}', with hardware '{}'",
            user.long_name,
            user.short_name,
            user.id,
            user.hw_model().as_str_name()
        )
        .into()
    }
}

impl Content {
    pub fn new() -> Self {
        Self::default()
    }

    #[allow(dead_code)]
    pub fn push<V>(mut self, value: V) -> Self
    where
        V: Into<ContentChunk>,
    {
        self.inner.push(value.into());
        self
    }

    pub fn view<'a, F>(&'a self, text_style_fn: &'a F) -> Row<'a, Message>
    where
        F: Fn(&Theme) -> Style,
    {
        let mut content_row = Row::new();
        for chunk in self.inner.iter() {
            match chunk {
                ContentChunk::Text(plain_text) => {
                    content_row = content_row.push(text(plain_text).style(text_style_fn).size(18))
                }
                ContentChunk::Link(link_text, url) => {
                    content_row = content_row.push(
                        iced::widget::button(text(link_text).style(text_style_fn).size(18))
                            .style(button_chip_style)
                            .on_press(OpenUrl(url.to_string())),
                    )
                }
            }
        }
        content_row
    }
}

impl fmt::Display for Content {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        for chunk in self.inner.iter() {
            match chunk {
                ContentChunk::Text(text_text) => f.write_str(text_text)?,
                ContentChunk::Link(link_text, _url) => f.write_str(link_text)?,
            }
        }

        Ok(())
    }
}
