use iced::{
    widget::{button, column, container, progress_bar, row, scrollable, text},
    Element, Length,
};
use skylock_core::Result;
use std::collections::VecDeque;
use chrono::{DateTime, Utc};

#[derive(Debug, Clone)]
pub enum StatusMessage {
    Refresh,
    Clear,
    Close,
}

#[derive(Debug, Clone)]
struct StatusEntry {
    timestamp: DateTime<Utc>,
    message: String,
    level: StatusLevel,
}

#[derive(Debug, Clone, PartialEq)]
enum StatusLevel {
    Info,
    Warning,
    Error,
}

pub struct StatusView {
    current_status: String,
    progress: f32,
    history: VecDeque<StatusEntry>,
    max_history: usize,
}

impl StatusView {
    pub fn new() -> Self {
        Self {
            current_status: String::from("Ready"),
            progress: 0.0,
            history: VecDeque::with_capacity(100),
            max_history: 100,
        }
    }

    pub fn update_status(&mut self, status: String, progress: f32) {
        self.current_status = status.clone();
        self.progress = progress;
        self.add_entry(StatusEntry {
            timestamp: Utc::now(),
            message: status,
            level: StatusLevel::Info,
        });
    }

    pub fn add_warning(&mut self, message: String) {
        self.add_entry(StatusEntry {
            timestamp: Utc::now(),
            message,
            level: StatusLevel::Warning,
        });
    }

    pub fn add_error(&mut self, message: String) {
        self.add_entry(StatusEntry {
            timestamp: Utc::now(),
            message,
            level: StatusLevel::Error,
        });
    }

    fn add_entry(&mut self, entry: StatusEntry) {
        if self.history.len() >= self.max_history {
            self.history.pop_front();
        }
        self.history.push_back(entry);
    }

    pub fn view(&self) -> Element<StatusMessage> {
        let status_text = text(&self.current_status).size(20);
        let progress = progress_bar(0.0..=1.0, self.progress);

        let history = self.history.iter().map(|entry| {
            let color = match entry.level {
                StatusLevel::Info => iced::Color::from_rgb(0.0, 0.0, 0.0),
                StatusLevel::Warning => iced::Color::from_rgb(0.8, 0.8, 0.0),
                StatusLevel::Error => iced::Color::from_rgb(0.8, 0.0, 0.0),
            };

            row![
                text(entry.timestamp.format("%Y-%m-%d %H:%M:%S")).size(14),
                text(&entry.message).size(14).style(color)
            ]
            .spacing(10)
        });

        let history_view = scrollable(
            column(history.collect())
                .spacing(5)
                .padding(10)
        );

        let content = column![
            row![text("Backup Status").size(24)].spacing(10),
            status_text,
            progress,
            container(history_view)
                .height(Length::Fill)
                .width(Length::Fill),
            row![
                button("Refresh").on_press(StatusMessage::Refresh),
                button("Clear History").on_press(StatusMessage::Clear),
                button("Close").on_press(StatusMessage::Close)
            ]
            .spacing(10)
        ]
        .spacing(20)
        .padding(20);

        container(content)
            .width(Length::Fill)
            .height(Length::Fill)
            .center_x()
            .center_y()
            .into()
    }

    pub fn update(&mut self, message: StatusMessage) {
        match message {
            StatusMessage::Refresh => {
                // Trigger status refresh from the backup system
            }
            StatusMessage::Clear => {
                self.history.clear();
            }
            StatusMessage::Close => {
                // Handle window close
            }
        }
    }

    pub fn get_latest_entries(&self, count: usize) -> Vec<&StatusEntry> {
        self.history.iter().rev().take(count).collect()
    }
}
