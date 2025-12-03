use crate::Message;
use crate::Message::RemoveNotification;
use crate::channel_view_entry::ChannelViewEntry;
use iced::border::Radius;
use iced::widget::container::Style;
use iced::widget::{Container, Row, button};
use iced::{Background, Border, Color, Element, Theme};

/// A [Notification] can be one of two notification types:
/// - Error(summary, detail)
/// - Info(summary, detail)
pub enum Notification {
    Error(String, String),
    Info(String, String),
}

/// A collection of notifications that should be shown on screen
#[derive(Default)]
pub struct Notifications {
    next_id: usize,
    inner: Vec<(usize, Notification)>,
}

impl Notifications {
    /// Render any notifications that are active into an element to display at the top of the screen
    pub fn view(&self) -> Element<'_, Message> {
        let mut notifications = Row::new().padding(10);

        // TODO a box with color and a cancel button that removes this error
        // larger font for summary, detail can be unfolded
        for (id, notification) in &self.inner {
            match notification {
                Notification::Error(summary, details) => {
                    notifications =
                        notifications.push(Self::error_box(*id, summary.clone(), details.clone()));
                }
                Notification::Info(summary, details) => {
                    notifications =
                        notifications.push(Self::info_box(*id, summary.clone(), details.clone()));
                }
            }
        }

        notifications.into()
    }

    // TODO accept detail and make it in an expandable box
    fn error_box(id: usize, summary: String, _detail: String) -> Element<'static, Message> {
        let row = Row::new()
            .push(iced::widget::text(summary))
            .push(button("OK").on_press(RemoveNotification(id)))
            .push(ChannelViewEntry::menu_bar());

        Container::new(row)
            .padding([6, 12]) // adjust to taste
            .style(|_theme: &Theme| Style {
                text_color: Some(Color::WHITE),
                background: Some(Background::Color(Color::from_rgb8(0xE5, 0x2D, 0x2C))), // red
                border: Border {
                    radius: Radius::from(12.0), // rounded corners
                    width: 2.0,
                    color: Color::from_rgb8(0xFF, 0xD7, 0x00), // yellow
                },
                ..Default::default()
            })
            .into()
    }

    // TODO accept detail and make it in an expandable box
    fn info_box(id: usize, summary: String, _detail: String) -> Element<'static, Message> {
        let row = Row::new().push(iced::widget::text(summary));
        let row = row.push(button("OK").on_press(RemoveNotification(id)));

        Container::new(row)
            .padding([6, 12]) // adjust to taste
            .style(|_theme: &Theme| Style {
                text_color: Some(Color::WHITE),
                background: Some(Background::Color(Color::from_rgb8(0x00, 0x00, 0x00))), // black
                border: Border {
                    radius: Radius::from(12.0), // rounded corners
                    width: 2.0,
                    color: Color::WHITE,
                },
                ..Default::default()
            })
            .into()
    }

    /// Add a notification to the list of notifications to display at the top of the screen.
    /// Notifications are displayed in a list, with the most recent at the top.
    /// Each notification has a unique id, which is used to remove it from the list.
    pub fn add(&mut self, notification: Notification) {
        self.inner.push((self.next_id, notification));
        self.next_id += 1;
    }

    /// Remove a notification from the list of notifications displayed at the top of the screen.
    /// Use the unique id to identify it
    pub fn remove(&mut self, id: usize) {
        self.inner.retain(|item| item.0 != id);
    }
}
