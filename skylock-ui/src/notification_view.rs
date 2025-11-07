use iced::{
    widget::{button, column, container, row, scrollable, text},
    Element, Length, Theme,
};
use chrono::{DateTime, Utc};
use skylock_core::notifications::{LocalNotification, NotificationSeverity};
use crate::style;

#[derive(Debug, Clone)]
pub enum NotificationMessage {
    Dismiss(DateTime<Utc>),
    ClearAll,
    MarkAllRead,
}

pub struct NotificationView {
    notifications: Vec<LocalNotification>,
    unread_count: usize,
}

impl NotificationView {
    pub fn new() -> Self {
        Self {
            notifications: Vec::new(),
            unread_count: 0,
        }
    }

    pub fn update(&mut self, message: NotificationMessage) {
        match message {
            NotificationMessage::Dismiss(timestamp) => {
                if let Some(pos) = self.notifications.iter().position(|n| n.timestamp == timestamp) {
                    self.notifications.remove(pos);
                    if self.unread_count > 0 {
                        self.unread_count -= 1;
                    }
                }
            }
            NotificationMessage::ClearAll => {
                self.notifications.clear();
                self.unread_count = 0;
            }
            NotificationMessage::MarkAllRead => {
                self.unread_count = 0;
            }
        }
    }

    pub fn add_notification(&mut self, notification: LocalNotification) {
        self.notifications.push(notification);
        self.unread_count += 1;
    }

    pub fn view(&self) -> Element<NotificationMessage> {
        let title = text("Notifications")
            .size(20)
            .style(style::Text::default());

        let clear_all = button("Clear All")
            .on_press(NotificationMessage::ClearAll)
            .style(style::Button::Destructive);

        let mark_read = button("Mark All Read")
            .on_press(NotificationMessage::MarkAllRead)
            .style(style::Button::Secondary);

        let header = row![title, mark_read, clear_all]
            .spacing(10)
            .align_items(iced::Alignment::Center);

        let notifications = self.notifications.iter().map(|notification| {
            self.notification_row(notification)
        });

        let content = column![header]
            .push(
                scrollable(
                    column(notifications.collect())
                        .spacing(10)
                        .padding(10)
                )
                .height(Length::Fill)
            )
            .spacing(20)
            .padding(20);

        container(content)
            .width(Length::Fill)
            .height(Length::Fill)
            .style(style::Container::default())
            .into()
    }

    fn notification_row(&self, notification: &LocalNotification) -> Element<NotificationMessage> {
        let severity_color = match notification.severity {
            NotificationSeverity::Info => style::Text::Info,
            NotificationSeverity::Warning => style::Text::Warning,
            NotificationSeverity::Error => style::Text::Error,
            NotificationSeverity::Critical => style::Text::Critical,
        };

        let title = text(&notification.title)
            .size(16)
            .style(severity_color);

        let message = text(&notification.message)
            .size(14)
            .style(style::Text::default());

        let timestamp = text(notification.timestamp.format("%H:%M:%S").to_string())
            .size(12)
            .style(style::Text::Subtle);

        let dismiss = button("×")
            .on_press(NotificationMessage::Dismiss(notification.timestamp))
            .style(style::Button::Transparent);

        row![
            column![title, message].spacing(5),
            row![timestamp, dismiss].spacing(10),
        ]
        .spacing(10)
        .align_items(iced::Alignment::Center)
        .into()
    }

    pub fn unread_count(&self) -> usize {
        self.unread_count
    }

    pub fn get_recent_notifications(&self, count: usize) -> Vec<&LocalNotification> {
        self.notifications.iter()
            .rev()
            .take(count)
            .collect()
    }

    pub fn get_notifications_by_severity(&self, severity: NotificationSeverity) -> Vec<&LocalNotification> {
        self.notifications.iter()
            .filter(|n| n.severity == severity)
            .collect()
    }
}
