use iced::{
    widget::{button, column, container, progress_bar, row, text},
    Application, Command, Element, Length, Settings, Subscription, Theme,
};
use skylock_core::{Result, SkylockError, notifications::NotificationManager};
use std::sync::Arc;
use tokio::sync::Mutex;
use tray_icon::{
    menu::{Menu, MenuEvent, MenuItem},
    TrayIcon, TrayIconBuilder,
};
use tokio::sync::mpsc;

#[derive(Debug, Clone)]
pub enum Message {
    StatusUpdate(String),
    ProgressUpdate(f32),
    OpenSettings,
    ShowBackupStatus,
    Exit,
    TrayEvent(TrayEventType),
}

#[derive(Debug, Clone)]
pub enum TrayEventType {
    LeftClick,
    RightClick,
    DoubleClick,
}

pub struct SkylockUI {
    notifications: NotificationManager,
    status: String,
    progress: f32,
    tray_icon: Option<TrayIcon>,
    tray_tx: mpsc::Sender<Message>,
    tray_rx: Arc<Mutex<mpsc::Receiver<Message>>>,
    window_visible: bool,
}

impl SkylockUI {
    pub fn new(notifications: NotificationManager) -> (Self, Command<Message>) {
        let (tray_tx, tray_rx) = mpsc::channel(32);

        (
            Self {
                notifications,
                status: String::from("Initializing..."),
                progress: 0.0,
                tray_icon: None,
                tray_tx,
                tray_rx: Arc::new(Mutex::new(tray_rx)),
                window_visible: false,
            },
            Command::none(),
        )
    }

    fn setup_tray(&mut self) -> Result<()> {
        let tray_menu = Menu::new();
        tray_menu.append(&MenuItem::new("Show Status", true, None));
        tray_menu.append(&MenuItem::new("Settings", true, None));
        tray_menu.append(&MenuItem::new("Exit", true, None));

        let icon = include_bytes!("../assets/skylock.ico");
        let tray_icon = TrayIconBuilder::new()
            .with_menu(Box::new(tray_menu))
            .with_tooltip("Skylock Backup")
            .with_icon(icon.to_vec())
            .build()?;

        self.tray_icon = Some(tray_icon);
        Ok(())
    }
}

impl Application for SkylockUI {
    type Message = Message;
    type Theme = Theme;
    type Executor = iced::executor::Default;
    type Flags = ();

    fn new(_flags: ()) -> (Self, Command<Message>) {
        let notifications = NotificationManager::new()
            .map_err(|e| SkylockError::System(SystemErrorType::GuiInitError.into()))
            .unwrap_or_else(|_| NotificationManager::default());
        Self::new(notifications)
    }

    fn title(&self) -> String {
        String::from("Skylock Backup")
    }

    fn update(&mut self, message: Message) -> Command<Message> {
        match message {
            Message::StatusUpdate(status) => {
                self.status = status;
                if let Err(e) = self.notifications.notify_backup_progress(self.status.clone(), (self.progress * 100.0) as u32) {
                    log::error!("Failed to send notification: {}", e);
                }
                Command::none()
            }
            Message::ProgressUpdate(progress) => {
                self.progress = progress;
                Command::none()
            }
            Message::OpenSettings => {
                // Open settings window
                self.window_visible = true;
                Command::none()
            }
            Message::ShowBackupStatus => {
                // Show backup status window
                self.window_visible = true;
                Command::none()
            }
            Message::Exit => {
                // Cleanup and exit
                if let Some(tray_icon) = self.tray_icon.take() {
                    drop(tray_icon);
                }
                std::process::exit(0);
            }
            Message::TrayEvent(event) => {
                match event {
                    TrayEventType::LeftClick => {
                        self.window_visible = !self.window_visible;
                    }
                    TrayEventType::RightClick => {}
                    TrayEventType::DoubleClick => {
                        self.window_visible = true;
                    }
                }
                Command::none()
            }
        }
    }

    fn view(&self) -> Element<Message> {
        if !self.window_visible {
            return container(text("")).into();
        }

        let status = text(&self.status).size(20);
        let progress = progress_bar(0.0..=1.0, self.progress);

        let content = column![
            row![text("Skylock Backup").size(24)].spacing(10),
            status,
            progress,
            row![
                button("Settings").on_press(Message::OpenSettings),
                button("Exit").on_press(Message::Exit),
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

    fn subscription(&self) -> Subscription<Message> {
        let tray_rx = self.tray_rx.clone();
        iced::subscription::unfold(
            "tray-events",
            tray_rx,
            move |rx| async move {
                let mut rx = rx.lock().await;
                if let Some(message) = rx.recv().await {
                    (message, rx)
                } else {
                    (Message::Exit, rx)
                }
            },
        )
    }
}
