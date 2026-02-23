//! Maildir storage adapter

use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

use async_trait::async_trait;
use tracing::{debug, warn};

use crate::storage::adapter::{MessageInfo, StorageAdapter, StorageError};

/// Maildir storage adapter
pub struct MaildirAdapter {
    base_path: PathBuf,
}

impl MaildirAdapter {
    /// Create a new Maildir adapter
    pub fn new<P: AsRef<Path>>(path: P) -> Result<Self, StorageError> {
        let base_path = path.as_ref().to_path_buf();

        // Ensure the maildir structure exists
        Self::ensure_maildir_structure(&base_path)?;

        Ok(Self { base_path })
    }

    /// Ensure the maildir directory structure exists
    fn ensure_maildir_structure(base: &Path) -> Result<(), StorageError> {
        let new_dir = base.join("new");
        let cur_dir = base.join("cur");
        let tmp_dir = base.join("tmp");

        for dir in &[&new_dir, &cur_dir, &tmp_dir] {
            if !dir.exists() {
                fs::create_dir_all(dir)?;
            }
        }

        Ok(())
    }

    /// Get the cur directory path
    fn cur_dir(&self) -> PathBuf {
        self.base_path.join("cur")
    }

    /// Get the new directory path
    fn new_dir(&self) -> PathBuf {
        self.base_path.join("new")
    }

    /// Generate a UIDL from filename
    fn generate_uidl(filename: &str) -> String {
        // Use MD5 hash of filename as UIDL for stability
        let digest = md5::compute(filename.as_bytes());
        hex::encode(digest.0)
    }

    /// Extract base filename (without flags)
    fn base_filename(filename: &str) -> &str {
        // Maildir filename format: unique.flags
        // The colon separates the base name from flags
        filename.split(':').next().unwrap_or(filename)
    }
}

#[async_trait]
impl StorageAdapter for MaildirAdapter {
    async fn list_messages(&self, _user: &str) -> Result<Vec<MessageInfo>, StorageError> {
        let cur_dir = self.cur_dir();
        let mut messages = Vec::new();

        if !cur_dir.exists() {
            return Ok(messages);
        }

        let entries = fs::read_dir(&cur_dir)?;

        for entry in entries {
            let entry = entry?;
            let path = entry.path();

            if path.is_file() {
                let filename = entry.file_name().to_string_lossy().to_string();
                let metadata = fs::metadata(&path)?;
                let size = metadata.len() as usize;
                let uidl = Self::generate_uidl(Self::base_filename(&filename));

                messages.push(MessageInfo {
                    id: filename,
                    size,
                    uidl,
                });
            }
        }

        // Sort by filename for consistent ordering
        messages.sort_by(|a, b| a.id.cmp(&b.id));

        debug!("Listed {} messages in maildir", messages.len());
        Ok(messages)
    }

    async fn get_message(&self, _user: &str, id: &str) -> Result<Vec<u8>, StorageError> {
        let path = self.cur_dir().join(id);

        if !path.exists() {
            return Err(StorageError::NotFound(id.to_string()));
        }

        let mut file = fs::File::open(&path)?;
        let mut content = Vec::new();
        file.read_to_end(&mut content)?;

        debug!("Retrieved message {} ({} bytes)", id, content.len());
        Ok(content)
    }

    async fn delete_message(&self, _user: &str, id: &str) -> Result<(), StorageError> {
        let path = self.cur_dir().join(id);

        if !path.exists() {
            return Err(StorageError::NotFound(id.to_string()));
        }

        fs::remove_file(&path)?;
        debug!("Deleted message {}", id);
        Ok(())
    }

    async fn process_new_messages(&self, _user: &str) -> Result<(), StorageError> {
        let new_dir = self.new_dir();
        let cur_dir = self.cur_dir();

        if !new_dir.exists() {
            return Ok(());
        }

        let entries = fs::read_dir(&new_dir)?;

        for entry in entries {
            let entry = entry?;
            let old_path = entry.path();

            if old_path.is_file() {
                let filename = entry.file_name();
                let new_path = cur_dir.join(&filename);

                // Move from new/ to cur/
                if let Err(e) = fs::rename(&old_path, &new_path) {
                    warn!("Failed to move message from new to cur: {}", e);
                } else {
                    debug!("Moved message {:?} from new to cur", filename);
                }
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::TempDir;

    fn setup_test_maildir() -> (TempDir, MaildirAdapter) {
        let temp_dir = TempDir::new().unwrap();
        let adapter = MaildirAdapter::new(temp_dir.path()).unwrap();
        (temp_dir, adapter)
    }

    #[tokio::test]
    async fn test_empty_maildir() {
        let (_temp_dir, adapter) = setup_test_maildir();
        let messages = adapter.list_messages("test").await.unwrap();
        assert!(messages.is_empty());
    }

    #[tokio::test]
    async fn test_list_messages() {
        let (temp_dir, adapter) = setup_test_maildir();

        // Create a test message
        let cur_dir = temp_dir.path().join("cur");
        let msg_path = cur_dir.join("1234567890.12345.hostname");
        let mut file = fs::File::create(&msg_path).unwrap();
        file.write_all(b"Subject: Test\r\n\r\nHello, World!")
            .unwrap();

        let messages = adapter.list_messages("test").await.unwrap();
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].id, "1234567890.12345.hostname");
    }

    #[tokio::test]
    async fn test_get_message() {
        let (temp_dir, adapter) = setup_test_maildir();

        let cur_dir = temp_dir.path().join("cur");
        let msg_path = cur_dir.join("test.msg");
        let content = b"Subject: Test\r\n\r\nHello, World!";
        fs::write(&msg_path, content).unwrap();

        let retrieved = adapter.get_message("test", "test.msg").await.unwrap();
        assert_eq!(retrieved, content);
    }

    #[tokio::test]
    async fn test_delete_message() {
        let (temp_dir, adapter) = setup_test_maildir();

        let cur_dir = temp_dir.path().join("cur");
        let msg_path = cur_dir.join("test.msg");
        fs::write(&msg_path, b"content").unwrap();

        assert!(msg_path.exists());
        adapter.delete_message("test", "test.msg").await.unwrap();
        assert!(!msg_path.exists());
    }

    #[tokio::test]
    async fn test_process_new_messages() {
        let (temp_dir, adapter) = setup_test_maildir();

        let new_dir = temp_dir.path().join("new");
        let cur_dir = temp_dir.path().join("cur");
        let msg_path = new_dir.join("new.msg");
        fs::write(&msg_path, b"content").unwrap();

        assert!(msg_path.exists());
        adapter.process_new_messages("test").await.unwrap();
        assert!(!msg_path.exists());
        assert!(cur_dir.join("new.msg").exists());
    }
}
