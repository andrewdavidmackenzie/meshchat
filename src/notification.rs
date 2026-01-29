use crate::Message;
use crate::Message::RemoveNotification;
use crate::styles::{
    TIME_TEXT_COLOR, TIME_TEXT_SIZE, TIME_TEXT_WIDTH, button_chip_style, error_notification_style,
    info_notification_style,
};
use chrono::{DateTime, Local, Utc};
use iced::Length::Fixed;
use iced::widget::container::Style;
use iced::widget::{Column, Container, Row, Text, button, text};
use iced::{Element, Fill, Renderer, Right, Task, Theme};

/// A [Notification] can be one of two notification types:
/// - Error(summary, detail)
/// - Info(summary, detail)
pub enum Notification {
    Error(String, String, u32), // Message, Detail, rx_time
    Info(String, String, u32),  // Message, Detail, rx_time
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
                Notification::Error(summary, details, rx_time) => Self::notification_box(
                    *id,
                    summary,
                    details,
                    *rx_time,
                    error_notification_style,
                ),
                Notification::Info(summary, details, rx_time) => {
                    Self::notification_box(*id, summary, details, *rx_time, info_notification_style)
                }
            });
        }

        notifications.into()
    }

    fn notification_box<'a>(
        id: usize,
        summary: &'a str,
        detail: &'a str,
        rx_time: u32,
        style: impl Fn(&Theme) -> Style + 'static,
    ) -> Element<'a, Message> {
        let top_row = Row::new().width(Fill).push(text(summary).size(20)).push(
            Column::new().width(Fill).align_x(Right).push(
                button("OK")
                    .style(button_chip_style)
                    .on_press(RemoveNotification(id)),
            ),
        );

        let bottom_row = Row::new()
            .push(text(detail).size(14))
            .push(Self::time_to_text(rx_time));

        Container::new(Column::new().push(top_row).push(bottom_row))
            .padding([6, 12])
            .style(style)
            .width(Fill)
            .into()
    }

    /// Convert the notification creation time to a Text element
    fn time_to_text<'a>(rx_time: u32) -> Text<'a, Theme, Renderer> {
        let datetime_utc = DateTime::<Utc>::from_timestamp_secs(rx_time as i64).unwrap();
        let datetime_local = datetime_utc.with_timezone(&Local);
        let time_str = datetime_local.format("%H:%M").to_string(); // Formats as HH:MM
        text(time_str)
            .color(TIME_TEXT_COLOR)
            .size(TIME_TEXT_SIZE)
            .width(Fixed(TIME_TEXT_WIDTH))
    }

    /// Add a notification to the list of notifications to display at the top of the screen.
    /// Notifications are displayed in a list, with the most recent at the top.
    /// Each notification has a unique id, which is used to remove it from the list.
    pub fn add(&mut self, notification: Notification) -> Task<Message> {
        self.inner.push((self.next_id, notification));
        self.next_id += 1;
        Task::none()
    }

    /// Remove a notification from the list of notifications displayed at the top of the screen.
    /// Use the unique id to identify it
    pub fn remove(&mut self, id: usize) -> Task<Message> {
        self.inner.retain(|item| item.0 != id);
        Task::none()
    }
}

#[cfg(test)]
mod tests {
    use crate::MeshChat;
    use crate::notification::{Notification, Notifications};

    #[test]
    fn test_empty_initially() {
        let notifications = Notifications::default();
        assert!(notifications.inner.is_empty());
    }

    #[test]
    fn test_add() {
        let mut notifications = Notifications::default();
        let _ = notifications.add(Notification::Info(
            "test".into(),
            "test".into(),
            MeshChat::now(),
        ));
        assert_eq!(notifications.inner.len(), 1);
    }

    #[test]
    fn test_remove_empty() {
        let mut notifications = Notifications::default();
        let _ = notifications.remove(0);
        assert_eq!(notifications.inner.len(), 0);
    }

    #[test]
    fn test_add_and_remove() {
        let mut notifications = Notifications::default();
        let _ = notifications.add(Notification::Info(
            "test1".into(),
            "test1".into(),
            MeshChat::now(),
        ));
        let _ = notifications.add(Notification::Info(
            "test2".into(),
            "test2".into(),
            MeshChat::now(),
        ));
        let _ = notifications.remove(0);
        assert_eq!(notifications.inner.len(), 1);
    }

    #[test]
    fn test_add_error_notification() {
        let mut notifications = Notifications::default();
        let _ = notifications.add(Notification::Error(
            "Error summary".into(),
            "Error detail".into(),
            MeshChat::now(),
        ));
        assert_eq!(notifications.inner.len(), 1);

        // Check it's stored with correct id
        assert_eq!(notifications.inner[0].0, 0);
    }

    #[test]
    fn test_ids_increment() {
        let mut notifications = Notifications::default();

        let _ = notifications.add(Notification::Info("1".into(), "1".into(), 0));
        let _ = notifications.add(Notification::Info("2".into(), "2".into(), 0));
        let _ = notifications.add(Notification::Info("3".into(), "3".into(), 0));

        assert_eq!(notifications.inner[0].0, 0);
        assert_eq!(notifications.inner[1].0, 1);
        assert_eq!(notifications.inner[2].0, 2);
        assert_eq!(notifications.next_id, 3);
    }

    #[test]
    fn test_remove_specific_id() {
        let mut notifications = Notifications::default();

        let _ = notifications.add(Notification::Info("1".into(), "1".into(), 0));
        let _ = notifications.add(Notification::Info("2".into(), "2".into(), 0));
        let _ = notifications.add(Notification::Info("3".into(), "3".into(), 0));

        // Remove the middle one (id 1)
        let _ = notifications.remove(1);

        assert_eq!(notifications.inner.len(), 2);
        // Should still have ids 0 and 2
        assert_eq!(notifications.inner[0].0, 0);
        assert_eq!(notifications.inner[1].0, 2);
    }

    #[test]
    fn test_remove_nonexistent_id() {
        let mut notifications = Notifications::default();

        let _ = notifications.add(Notification::Info("1".into(), "1".into(), 0));

        // Remove an id that doesn't exist
        let _ = notifications.remove(999);

        // Should still have the original
        assert_eq!(notifications.inner.len(), 1);
    }

    #[test]
    fn test_remove_all() {
        let mut notifications = Notifications::default();

        let _ = notifications.add(Notification::Info("1".into(), "1".into(), 0));
        let _ = notifications.add(Notification::Info("2".into(), "2".into(), 0));

        let _ = notifications.remove(0);
        let _ = notifications.remove(1);

        assert!(notifications.inner.is_empty());
    }

    #[test]
    fn test_mixed_notification_types() {
        let mut notifications = Notifications::default();

        let _ = notifications.add(Notification::Info("info".into(), "detail".into(), 0));
        let _ = notifications.add(Notification::Error("error".into(), "detail".into(), 0));
        let _ = notifications.add(Notification::Info("info2".into(), "detail".into(), 0));

        assert_eq!(notifications.inner.len(), 3);
    }

    #[test]
    fn test_default_next_id_is_zero() {
        let notifications = Notifications::default();
        assert_eq!(notifications.next_id, 0);
    }
}
