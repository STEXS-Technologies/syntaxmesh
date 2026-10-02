use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, SyncSender};
use std::time::Duration;

use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};

#[cfg(test)]
mod tests;

/// An inventory rescan request; paths are deliberately not retained.
pub struct WatchNotice {
    /// Sticky: at least one backend error has occurred since registration.
    pub degraded: bool,
}

/// Native notifications with one queued inventory invalidation.
pub struct FilesystemNotifications {
    _watcher: RecommendedWatcher,
    receiver: Receiver<()>,
    degraded: Arc<AtomicBool>,
}

impl FilesystemNotifications {
    /// Registers recursive notifications before a host's initial inventory scan.
    /// Exclusions must be absolute paths; descendants are excluded as well.
    ///
    /// # Errors
    /// Returns an error for invalid configuration or backend registration failure.
    pub fn open(root: &Path, exclusions: Vec<PathBuf>) -> notify::Result<Self> {
        if !root.is_absolute() || exclusions.iter().any(|path| !path.is_absolute()) {
            return Err(notify::Error::generic("watch paths must be absolute"));
        }
        let (sender, receiver) = mpsc::sync_channel(1);
        let degraded = Arc::new(AtomicBool::new(false));
        let callback_degraded = Arc::clone(&degraded);
        let mut watcher = notify::recommended_watcher(move |event| {
            enqueue(event, &exclusions, &sender, &callback_degraded);
        })?;
        watcher.watch(root, RecursiveMode::Recursive)?;
        Ok(Self {
            _watcher: watcher,
            receiver,
            degraded,
        })
    }

    /// Waits for one invalidation; timeout does not indicate watcher failure.
    ///
    /// # Errors
    /// Returns Disconnected if the notification callback has stopped.
    pub fn receive(&self, timeout: Duration) -> Result<Option<WatchNotice>, RecvTimeoutError> {
        match self.receiver.recv_timeout(timeout) {
            Ok(()) => Ok(Some(WatchNotice {
                degraded: self.degraded.load(Ordering::Acquire),
            })),
            Err(RecvTimeoutError::Timeout) => Ok(None),
            Err(error) => Err(error),
        }
    }
}

fn enqueue(
    event: notify::Result<Event>,
    exclusions: &[PathBuf],
    sender: &SyncSender<()>,
    degraded: &AtomicBool,
) {
    match event {
        Ok(event) if !event.need_rescan() => {
            if matches!(event.kind, EventKind::Access(_))
                || (!event.paths.is_empty()
                    && event
                        .paths
                        .iter()
                        .all(|path| exclusions.iter().any(|excluded| path.starts_with(excluded))))
            {
                return;
            }
        }
        Ok(_) => {}
        Err(_) => degraded.store(true, Ordering::Release),
    }
    // Full means another invalidation is already pending; disconnected means
    // the host has dropped the receiver. Neither should block the OS callback.
    let _delivery = sender.try_send(());
}
