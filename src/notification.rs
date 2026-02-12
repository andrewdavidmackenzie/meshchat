use crate::Message;
use crate::Message::RemoveNotification;
use crate::styles::{
    TIME_TEXT_COLOR, TIME_TEXT_SIZE, TIME_TEXT_WIDTH, button_chip_style, error_notification_style,
    info_notification_style, permanent_notification_style,
};
use chrono::{DateTime, Local, Utc};
use iced::Length::Fixed;
use iced::widget::container::Style;
use iced::widget::{Column, Container, Row, Space, Text, button, text};
use iced::{Bottom, Element, Fill, Renderer, Right, Task, Theme};

/// A [Notification] can be one of two notification types:
/// - Error(summary, detail)
/// - Info(summary, detail)
#[derive(Debug)]
pub enum Notification {
    Critical(String, String, u32), // Message, Detail, rx_time
    Error(String, String, u32),    // Message, Detail, rx_time
    Info(String, String, u32),     // Message, Detail, rx_time
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
        let mut notifications = Column::new().padding(10);

        for (id, notification) in &self.inner {
            let (style, cancellable, summary, details, rx_time) = match notification {
                Notification::Critical(s, d, t) => (
                    permanent_notification_style as fn(&Theme) -> Style,
                    false,
                    s,
                    d,
                    t,
                ),
                Notification::Error(s, d, t) => (
                    error_notification_style as fn(&Theme) -> Style,
                    true,
                    s,
                    d,
                    t,
                ),
                Notification::Info(s, d, t) => (
                    info_notification_style as fn(&Theme) -> Style,
                    true,
                    s,
                    d,
                    t,
                ),
            };

            notifications = notifications.push(Self::notification_box(
                *id,
                summary,
                details,
                *rx_time,
                style,
                cancellable,
            ));
        }

        notifications.into()
    }

    fn notification_box<'a>(
        id: usize,
        summary: &'a str,
        detail: &'a str,
        rx_time: u32,
        style: impl Fn(&Theme) -> Style + 'static,
        cancellable: bool,
    ) -> Element<'a, Message> {
        let mut top_row = Row::new().width(Fill).push(text(summary).size(20));
        if cancellable {
            top_row = top_row.push(
                Column::new().width(Fill).align_x(Right).push(
                    button("OK")
                        .style(button_chip_style)
                        .on_press(RemoveNotification(id)),
                ),
            );
        }

        let bottom_row = Row::new()
            .push(text(detail).size(14))
            .push(Space::new().width(Fill))
            .push(Self::time_to_text(rx_time))
            .align_y(Bottom);

        Container::new(Column::new().push(top_row).push(bottom_row))
            .padding([6, 12])
            .style(style)
            .width(Fill)
            .into()
    }

    /// Convert the notification creation time to a Text element
    fn time_to_text<'a>(rx_time: u32) -> Text<'a, Theme, Renderer> {
        let datetime_utc = DateTime::<Utc>::from_timestamp_secs(rx_time as i64).unwrap_or_default();
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

        // Check it's stored with the correct id
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

    #[test]
    fn test_notification_with_empty_strings() {
        let mut notifications = Notifications::default();
        let _ = notifications.add(Notification::Info("".into(), "".into(), 0));
        assert_eq!(notifications.inner.len(), 1);
    }

    #[test]
    fn test_notification_with_long_strings() {
        let mut notifications = Notifications::default();
        let long_summary = "A".repeat(1000);
        let long_detail = "B".repeat(5000);
        let _ = notifications.add(Notification::Error(long_summary, long_detail, 0));
        assert_eq!(notifications.inner.len(), 1);
    }

    #[test]
    fn test_notification_with_unicode() {
        let mut notifications = Notifications::default();
        let _ = notifications.add(Notification::Info(
            "⚠️ Warning".into(),
            "日本語のテキスト".into(),
            0,
        ));
        assert_eq!(notifications.inner.len(), 1);
    }

    #[test]
    fn test_add_many_notifications() {
        let mut notifications = Notifications::default();
        for i in 0..100 {
            let _ = notifications.add(Notification::Info(
                format!("Notification {}", i),
                format!("Detail {}", i),
                i as u32,
            ));
        }
        assert_eq!(notifications.inner.len(), 100);
        assert_eq!(notifications.next_id, 100);
    }

    #[test]
    fn test_remove_first_notification() {
        let mut notifications = Notifications::default();
        let _ = notifications.add(Notification::Info("1".into(), "1".into(), 0));
        let _ = notifications.add(Notification::Info("2".into(), "2".into(), 0));
        let _ = notifications.add(Notification::Info("3".into(), "3".into(), 0));

        let _ = notifications.remove(0);

        assert_eq!(notifications.inner.len(), 2);
        assert_eq!(notifications.inner[0].0, 1);
    }

    #[test]
    fn test_remove_last_notification() {
        let mut notifications = Notifications::default();
        let _ = notifications.add(Notification::Info("1".into(), "1".into(), 0));
        let _ = notifications.add(Notification::Info("2".into(), "2".into(), 0));
        let _ = notifications.add(Notification::Info("3".into(), "3".into(), 0));

        let _ = notifications.remove(2);

        assert_eq!(notifications.inner.len(), 2);
        assert_eq!(notifications.inner[0].0, 0);
        assert_eq!(notifications.inner[1].0, 1);
    }

    #[test]
    fn test_notification_rx_time_preserved() {
        let mut notifications = Notifications::default();
        let rx_time = 1234567890;
        let _ = notifications.add(Notification::Info("test".into(), "detail".into(), rx_time));

        let notification = &notifications.inner[0].1;
        assert!(
            matches!(notification, Notification::Info(_, _, time) if *time == rx_time),
            "Info notification should preserve rx_time {}, got {:?}",
            rx_time,
            notification
        );
    }

    #[test]
    fn test_error_notification_rx_time_preserved() {
        let mut notifications = Notifications::default();
        let rx_time = 1234567890;
        let _ = notifications.add(Notification::Error(
            "error".into(),
            "detail".into(),
            rx_time,
        ));

        let notification = &notifications.inner[0].1;
        assert!(
            matches!(notification, Notification::Error(_, _, time) if *time == rx_time),
            "Error notification should preserve rx_time {}, got {:?}",
            rx_time,
            notification
        );
    }

    #[test]
    fn test_ids_continue_after_remove() {
        let mut notifications = Notifications::default();

        let _ = notifications.add(Notification::Info("1".into(), "1".into(), 0));
        let _ = notifications.add(Notification::Info("2".into(), "2".into(), 0));

        let _ = notifications.remove(0);
        let _ = notifications.remove(1);

        // Add a new notification - ID should continue from where it left off
        let _ = notifications.add(Notification::Info("3".into(), "3".into(), 0));
        assert_eq!(notifications.inner[0].0, 2);
        assert_eq!(notifications.next_id, 3);
    }

    #[test]
    fn test_notification_summary_and_detail_preserved() {
        let mut notifications = Notifications::default();
        let _ = notifications.add(Notification::Info("Summary".into(), "Detail".into(), 0));

        let notification = &notifications.inner[0].1;
        assert!(
            matches!(notification, Notification::Info(s, d, _) if s == "Summary" && d == "Detail"),
            "Info notification should preserve summary 'Summary' and detail 'Detail', got {:?}",
            notification
        );
    }

    #[test]
    fn test_error_summary_and_detail_preserved() {
        let mut notifications = Notifications::default();
        let _ = notifications.add(Notification::Error(
            "Error Summary".into(),
            "Error Detail".into(),
            0,
        ));

        let notification = &notifications.inner[0].1;
        assert!(
            matches!(notification, Notification::Error(s, d, _) if s == "Error Summary" && d == "Error Detail"),
            "Error notification should preserve summary 'Error Summary' and detail 'Error Detail', got {:?}",
            notification
        );
    }

    // Tests for view functions - verify they return Elements without panicking

    #[test]
    fn test_view_empty_notifications() {
        let notifications = Notifications::default();
        let _element = notifications.view();
        // Should not panic and return an Element
    }

    #[test]
    fn test_view_single_info_notification() {
        let mut notifications = Notifications::default();
        let _ = notifications.add(Notification::Info(
            "Info Summary".into(),
            "Info Detail".into(),
            1234567890,
        ));
        let _element = notifications.view();
        // Should not panic
    }

    #[test]
    fn test_view_single_error_notification() {
        let mut notifications = Notifications::default();
        let _ = notifications.add(Notification::Error(
            "Error Summary".into(),
            "Error Detail".into(),
            1234567890,
        ));
        let _element = notifications.view();
        // Should not panic
    }

    #[test]
    fn test_view_multiple_notifications() {
        let mut notifications = Notifications::default();
        let _ = notifications.add(Notification::Info("Info 1".into(), "Detail 1".into(), 0));
        let _ = notifications.add(Notification::Error(
            "Error 1".into(),
            "Detail 2".into(),
            100,
        ));
        let _ = notifications.add(Notification::Info("Info 2".into(), "Detail 3".into(), 200));
        let _element = notifications.view();
        // Should not panic
    }

    #[test]
    fn test_view_with_empty_strings() {
        let mut notifications = Notifications::default();
        let _ = notifications.add(Notification::Info("".into(), "".into(), 0));
        let _element = notifications.view();
        // Should not panic with empty strings
    }

    #[test]
    fn test_view_with_long_strings() {
        let mut notifications = Notifications::default();
        let long_summary = "A".repeat(1000);
        let long_detail = "B".repeat(5000);
        let _ = notifications.add(Notification::Info(long_summary, long_detail, 0));
        let _element = notifications.view();
        // Should not panic with long strings
    }

    #[test]
    fn test_view_with_unicode() {
        let mut notifications = Notifications::default();
        let _ = notifications.add(Notification::Info(
            "⚠️ 警告 Warning".into(),
            "日本語 中文 한국어".into(),
            0,
        ));
        let _element = notifications.view();
        // Should not panic with Unicode
    }

    #[test]
    fn test_view_with_special_characters() {
        let mut notifications = Notifications::default();
        let _ = notifications.add(Notification::Info(
            "Line1\nLine2\tTabbed".into(),
            "Detail with\r\nnewlines".into(),
            0,
        ));
        let _element = notifications.view();
        // Should not panic with special characters
    }

    #[test]
    fn test_view_with_zero_rx_time() {
        let mut notifications = Notifications::default();
        let _ = notifications.add(Notification::Info("Test".into(), "Detail".into(), 0));
        let _element = notifications.view();
        // Should not panic with zero time (Unix epoch)
    }

    #[test]
    fn test_view_with_max_rx_time() {
        let mut notifications = Notifications::default();
        let _ = notifications.add(Notification::Info("Test".into(), "Detail".into(), u32::MAX));
        let _element = notifications.view();
        // Should not panic with max time value
    }

    #[test]
    fn test_view_with_current_time() {
        let mut notifications = Notifications::default();
        let _ = notifications.add(Notification::Info(
            "Test".into(),
            "Detail".into(),
            MeshChat::now(),
        ));
        let _element = notifications.view();
        // Should not panic with current time
    }

    #[test]
    fn test_time_to_text_zero() {
        let _text = Notifications::time_to_text(0);
        // Should not panic and return a Text element
    }

    #[test]
    fn test_time_to_text_current() {
        let _text = Notifications::time_to_text(MeshChat::now());
        // Should not panic
    }

    #[test]
    fn test_time_to_text_max() {
        let _text = Notifications::time_to_text(u32::MAX);
        // Should not panic with max value
    }

    #[test]
    fn test_time_to_text_various_times() {
        // Test various timestamps
        let timestamps = [0, 1, 100, 1000, 1000000, 1609459200, 1700000000];
        for ts in timestamps {
            let _text = Notifications::time_to_text(ts);
            // Should not panic
        }
    }

    #[test]
    fn test_notification_box_info_style() {
        use crate::styles::info_notification_style;
        let _element = Notifications::notification_box(
            0,
            "Summary",
            "Detail",
            MeshChat::now(),
            info_notification_style,
            true,
        );
        // Should not panic
    }

    #[test]
    fn test_notification_box_error_style() {
        use crate::styles::error_notification_style;
        let _element = Notifications::notification_box(
            0,
            "Summary",
            "Detail",
            MeshChat::now(),
            error_notification_style,
            true,
        );
        // Should not panic
    }

    #[test]
    fn test_notification_box_empty_strings() {
        use crate::styles::info_notification_style;
        let _element = Notifications::notification_box(0, "", "", 0, info_notification_style, true);
        // Should not panic with empty strings
    }

    #[test]
    fn test_notification_box_long_strings() {
        use crate::styles::info_notification_style;
        let long_summary = "A".repeat(500);
        let long_detail = "B".repeat(2000);
        let _element = Notifications::notification_box(
            0,
            &long_summary,
            &long_detail,
            0,
            info_notification_style,
            true,
        );
        // Should not panic
    }

    #[test]
    fn test_notification_box_various_ids() {
        use crate::styles::info_notification_style;
        let ids = [0, 1, 100, 1000, usize::MAX];
        for id in ids {
            let _element = Notifications::notification_box(
                id,
                "Summary",
                "Detail",
                0,
                info_notification_style,
                true,
            );
            // Should not panic
        }
    }

    #[test]
    fn test_view_after_add_and_remove() {
        let mut notifications = Notifications::default();

        // Add some
        let _ = notifications.add(Notification::Info("1".into(), "1".into(), 0));
        let _ = notifications.add(Notification::Error("2".into(), "2".into(), 0));
        {
            let _element = notifications.view();
            // Element dropped here
        }

        // Remove one
        let _ = notifications.remove(0);
        {
            let _element = notifications.view();
        }

        // Remove remaining
        let _ = notifications.remove(1);
        {
            let _element = notifications.view();
        }
        // Should not panic at any stage
    }

    #[test]
    fn test_view_many_notifications() {
        let mut notifications = Notifications::default();
        for i in 0..50 {
            let _ = notifications.add(Notification::Info(
                format!("Notification {}", i),
                format!("Detail {}", i),
                i as u32,
            ));
        }
        let _element = notifications.view();
        // Should not panic with many notifications
    }
}
