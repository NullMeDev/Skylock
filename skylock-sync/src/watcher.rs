use notify::{RecommendedWatcher, RecursiveMode, Watcher, Eve                        },
                        _ => Vec::new(),
                    };

                    for event in events {
                        if let Err(e) = tx.send(event).await {
                            eprintln!("Failed to send file event: {}", e);
                        }
                    }
                },
                Err(e) => {
                    if let Err(e) = tx.send(FileEvent::Error(e.to_string())).await {
                        eprintln!("Failed to send error event: {}", e);
                    }
                }, EventKind};
use tokio::sync::mpsc;
use std::path::{Path, PathBuf};
use std::time::Duration;
use std::collections::{HashMap, VecDeque};
use tokio::time::sleep;
use chrono::{DateTime, Utc};
use skylock_core::Result;

#[derive(Debug, Clone)]
pub enum FileEvent {
    Created(PathBuf),
    Modified(PathBuf),
    Deleted(PathBuf),
    Renamed(PathBuf, PathBuf),
    Error(String),
}

pub struct FileWatcher {
    watcher: RecommendedWatcher,
    event_tx: mpsc::Sender<FileEvent>,
    debounce_queue: HashMap<PathBuf, VecDeque<(DateTime<Utc>, FileEvent)>>,
    debounce_duration: Duration,
}

impl FileWatcher {
    pub fn new(event_tx: mpsc::Sender<FileEvent>, debounce_ms: u64) -> Result<Self> {
        let (event_internal_tx, mut event_internal_rx) = mpsc::channel(100);
        let event_tx_clone = event_tx.clone();

        // Start event processor task
        let tx = event_tx_clone.clone();
        tokio::spawn(async move {
            while let Some(event) = event_internal_rx.recv().await {
                if let Err(e) = tx.send(event).await {
                    eprintln!("Failed to forward file event: {}", e);
                }
            }
        });

        let watcher = notify::recommended_watcher(move |res: notify::Result<Event>| {
            let tx = event_internal_tx.clone();
            tokio::spawn(async move {
                match res {
                    Ok(event) => {
                        let events = match event.kind {
                            EventKind::Create(_) => {
                                event.paths.into_iter()
                                    .map(FileEvent::Created)
                                    .collect::<Vec<_>>()
                            },
                            EventKind::Modify(_) => {
                                event.paths.into_iter()
                                    .map(FileEvent::Modified)
                                    .collect::<Vec<_>>()
                            },
                            EventKind::Remove(_) => {
                                event.paths.into_iter()
                                    .map(FileEvent::Deleted)
                                    .collect::<Vec<_>>()
                            },
                            EventKind::Rename(_) => {
                                let mut paths = event.paths.into_iter();
                                match (paths.next(), paths.next()) {
                                    (Some(from), Some(to)) => vec![FileEvent::Renamed(from, to)],
                                    _ => Vec::new(), // Invalid rename event
                        if paths.len() == 2 {
                            vec![FileEvent::Renamed(paths[0].clone(), paths[1].clone())]
                        } else {
                            vec![FileEvent::Error("Invalid rename event".to_string())]
                        }
                    },
                    _ => vec![],
                },
                Err(e) => vec![FileEvent::Error(e.to_string())],
            };

            let tx = event_tx_clone.clone();
            tokio::spawn(async move {
                for e in evt {
                    let _ = tx.send(e).await;
                }
            });
        })?;

        Ok(Self {
            watcher,
            event_tx,
            debounce_queue: HashMap::new(),
            debounce_duration: Duration::from_millis(debounce_ms),
        })
    }

    pub fn watch<P: AsRef<Path>>(&mut self, path: P) -> Result<()> {
        self.watcher.watch(path.as_ref(), RecursiveMode::Recursive)?;
        Ok(())
    }

    pub async fn process_events(&mut self) -> Result<()> {
        loop {
            // Process debounce queue
            let now = Utc::now();
            let mut to_process = Vec::new();

            self.debounce_queue.retain(|_path, events| {
                while let Some((timestamp, _)) = events.front() {
                    match chrono::Duration::from_std(self.debounce_duration) {
                        Ok(duration) if now - *timestamp > duration => {
                            if let Some((_, event)) = events.pop_front() {
                                to_process.push(event);
                            }
                        }
                        _ => break,
                    }
                }
                !events.is_empty()
            });

            // Send processed events
            for event in to_process {
                self.event_tx.send(event).await?;
            }

            sleep(Duration::from_millis(100)).await;
        }
    }

    fn add_to_debounce_queue(&mut self, path: PathBuf, event: FileEvent) {
        let events = self.debounce_queue
            .entry(path)
            .or_insert_with(VecDeque::new);

        events.push_back((Utc::now(), event));

        // Keep only last 5 events per path
        while events.len() > 5 {
            events.pop_front();
        }
    }
}

#[derive(Debug)]
pub struct SyncCoordinator {
    file_watcher: FileWatcher,
    event_rx: mpsc::Receiver<FileEvent>,
    sync_tx: mpsc::Sender<SyncEvent>,
}

#[derive(Debug, Clone)]
pub enum SyncEvent {
    FileChanged {
        path: PathBuf,
        change_type: ChangeType,
        timestamp: DateTime<Utc>,
    },
    Error(String),
}

#[derive(Debug, Clone)]
pub enum ChangeType {
    Created,
    Modified,
    Deleted,
    Renamed(PathBuf),
}

impl SyncCoordinator {
    pub fn new(sync_tx: mpsc::Sender<SyncEvent>) -> Result<Self> {
        let (event_tx, event_rx) = mpsc::channel(1000);
        let file_watcher = FileWatcher::new(event_tx, 500)?;

        Ok(Self {
            file_watcher,
            event_rx,
            sync_tx,
        })
    }

    pub fn watch<P: AsRef<Path>>(&mut self, path: P) -> Result<()> {
        self.file_watcher.watch(path)?;
        Ok(())
    }

    pub async fn run(&mut self) -> Result<()> {
        // Start the file watcher processing loop
        let mut file_watcher = std::mem::replace(&mut self.file_watcher,
            FileWatcher::new(mpsc::channel(1).0, 500)?);

        tokio::spawn(async move {
            let _ = file_watcher.process_events().await;
        });

        // Process events and convert them to sync events
        while let Some(event) = self.event_rx.recv().await {
            let sync_event = match event {
                FileEvent::Created(path) => SyncEvent::FileChanged {
                    path,
                    change_type: ChangeType::Created,
                    timestamp: Utc::now(),
                },
                FileEvent::Modified(path) => SyncEvent::FileChanged {
                    path,
                    change_type: ChangeType::Modified,
                    timestamp: Utc::now(),
                },
                FileEvent::Deleted(path) => SyncEvent::FileChanged {
                    path,
                    change_type: ChangeType::Deleted,
                    timestamp: Utc::now(),
                },
                FileEvent::Renamed(old_path, new_path) => SyncEvent::FileChanged {
                    path: new_path,
                    change_type: ChangeType::Renamed(old_path),
                    timestamp: Utc::now(),
                },
                FileEvent::Error(err) => SyncEvent::Error(err),
            };

            self.sync_tx.send(sync_event).await?;
        }

        Ok(())
    }
}
