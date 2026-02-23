//! Mail watcher for detecting new messages and forwarding

use std::path::{Path, PathBuf};
use std::sync::Arc;

use notify::{Event, RecursiveMode, Watcher};
use tokio::sync::mpsc;
use tracing::{debug, error, info, warn};

use crate::auth::ImapForwardConfig;
use crate::imap_forward::ImapForwarder;

/// Mail watcher for monitoring and forwarding new messages
pub struct MailWatcher {
    maildir_path: PathBuf,
    forwarder: Arc<ImapForwarder>,
    delete_after_forward: bool,
}

impl MailWatcher {
    /// Create a new mail watcher
    pub fn new(
        maildir_path: PathBuf,
        config: ImapForwardConfig,
    ) -> Self {
        let delete_after_forward = config.delete_after_forward;
        let forwarder = Arc::new(ImapForwarder::new(config));

        Self {
            maildir_path,
            forwarder,
            delete_after_forward,
        }
    }

    /// Start watching for new messages
    pub async fn watch(&self) -> anyhow::Result<()> {
        if !self.forwarder.is_enabled() {
            return Ok(());
        }

        let new_dir = self.maildir_path.join("new");
        if !new_dir.exists() {
            std::fs::create_dir_all(&new_dir)?;
        }

        info!("Starting mail watcher for {:?}", self.maildir_path);

        let (tx, mut rx) = mpsc::channel::<PathBuf>(100);

        // Set up file watcher
        let tx_clone = tx.clone();
        let mut watcher = notify::recommended_watcher(move |res: Result<Event, notify::Error>| {
            match res {
                Ok(event) => {
                    if event.kind.is_create() || event.kind.is_modify() {
                        for path in event.paths {
                            if path.parent().map_or(false, |p| p.ends_with("new")) {
                                if let Err(e) = tx_clone.blocking_send(path) {
                                    error!("Failed to send path to channel: {}", e);
                                }
                            }
                        }
                    }
                }
                Err(e) => {
                    error!("Watch error: {}", e);
                }
            }
        })?;

        watcher.watch(&new_dir, RecursiveMode::NonRecursive)?;

        // Process incoming messages
        while let Some(path) = rx.recv().await {
            if let Err(e) = self.process_new_message(&path).await {
                warn!("Failed to process message {:?}: {}", path, e);
            }
        }

        Ok(())
    }

    /// Process a single new message
    async fn process_new_message(&self, path: &Path) -> anyhow::Result<()> {
        debug!("Processing new message: {:?}", path);

        // Read the message content
        let content = tokio::fs::read(path).await?;

        // Forward to IMAP server
        if let Err(e) = self.forwarder.forward(&content).await {
            error!("Failed to forward message: {}", e);
            return Err(e.into());
        }

        // Optionally delete the message after forwarding
        if self.delete_after_forward {
            tokio::fs::remove_file(path).await?;
            info!("Deleted message after forwarding: {:?}", path);
        } else {
            // Move to cur directory
            let filename = path.file_name().unwrap();
            let cur_path = self.maildir_path.join("cur").join(filename);
            tokio::fs::rename(path, &cur_path).await?;
            info!("Moved message to cur after forwarding: {:?}", cur_path);
        }

        Ok(())
    }

    /// Process all existing new messages (for startup)
    pub async fn process_existing(&self) -> anyhow::Result<()> {
        if !self.forwarder.is_enabled() {
            return Ok(());
        }

        let new_dir = self.maildir_path.join("new");
        if !new_dir.exists() {
            return Ok(());
        }

        let mut entries = tokio::fs::read_dir(&new_dir).await?;
        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();
            if path.is_file() {
                if let Err(e) = self.process_new_message(&path).await {
                    warn!("Failed to process existing message {:?}: {}", path, e);
                }
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_watcher_creation() {
        let temp_dir = TempDir::new().unwrap();
        let config = ImapForwardConfig::default();
        let watcher = MailWatcher::new(temp_dir.path().to_path_buf(), config);
        assert!(!watcher.forwarder.is_enabled());
    }
}
