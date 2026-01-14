use crate::Message;
use iced::Element;
use iced::widget::{Row, text};
use url::Url;

#[derive(Clone, Debug)]
pub enum ContentChunk {
    Text(String),
    Link(String, String),
}

#[derive(Clone, Debug)]
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

impl Content {
    #[allow(dead_code)]
    pub fn push<V>(mut self, value: V) -> Self
    where
        V: Into<ContentChunk>,
    {
        self.inner.push(value.into());
        self
    }

    pub fn view(&self) -> Element<'_, Message> {
        let mut detail_row = Row::new();
        for chunk in self.inner.iter() {
            match chunk {
                ContentChunk::Text(text) => detail_row = detail_row.push(iced::widget::text(text)),
                ContentChunk::Link(link_text, _url) => {
                    detail_row = detail_row.push(iced::widget::button(text(link_text)))
                }
            }
        }
        detail_row.into()
    }
}
