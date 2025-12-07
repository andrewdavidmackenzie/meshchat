use crate::Message;
use crate::Message::RemoveNotification;
use crate::styles::{button_chip_style, error_notification_style, info_notification_style};
use iced::widget::container::Style;
use iced::widget::{Column, Container, Row, button, text};
use iced::{Element, Fill, Right, Theme};

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

        for (id, notification) in &self.inner {
            notifications = notifications.push(match notification {
                Notification::Error(summary, details) => Self::notification_box(
                    *id,
                    summary.clone(),
                    details.clone(),
                    error_notification_style,
                ),
                Notification::Info(summary, details) => Self::notification_box(
                    *id,
                    summary.clone(),
                    details.clone(),
                    info_notification_style,
                ),
            });
        }

        notifications.into()
    }

    fn notification_box(
        id: usize,
        summary: String,
        detail: String,
        style: impl Fn(&Theme) -> Style + 'static,
    ) -> Element<'static, Message> {
        let row = Row::new()
            .width(iced::Length::Fill)
            .push(text(summary).size(20))
            .push(
                Column::new().width(Fill).align_x(Right).push(
                    button("OK")
                        .style(button_chip_style)
                        .on_press(RemoveNotification(id)),
                ),
            );

        Container::new(Column::new().push(row).push(text(detail).size(14)))
            .padding([6, 12])
            .style(style)
            .width(iced::Length::Fill)
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
