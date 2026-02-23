//! Storage adapter trait and factory

use async_trait::async_trait;
use thiserror::Error;

use crate::auth::User;
use crate::config::StorageConfig;
use crate::storage::maildir::MaildirAdapter;

#[cfg(feature = "s3")]
use crate::storage::s3::S3Adapter;

#[derive(Error, Debug)]
pub enum StorageError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Message not found: {0}")]
    NotFound(String),
    #[error("Storage error: {0}")]
    Internal(String),
    #[error("Configuration error: {0}")]
    Config(String),
}

/// Information about a stored message
#[derive(Debug, Clone)]
pub struct MessageInfo {
    /// Unique identifier for the message
    pub id: String,
    /// Size in bytes
    pub size: usize,
    /// Unique ID for UIDL command
    pub uidl: String,
}

/// Storage adapter trait for different storage backends
#[async_trait]
pub trait StorageAdapter: Send + Sync {
    /// List all messages for a user
    async fn list_messages(&self, user: &str) -> Result<Vec<MessageInfo>, StorageError>;

    /// Get the content of a specific message
    async fn get_message(&self, user: &str, id: &str) -> Result<Vec<u8>, StorageError>;

    /// Delete a message
    async fn delete_message(&self, user: &str, id: &str) -> Result<(), StorageError>;

    /// Process new messages (move from new/ to cur/ for Maildir)
    async fn process_new_messages(&self, user: &str) -> Result<(), StorageError>;
}

/// Factory for creating storage adapters
pub struct StorageFactory {
    config: StorageConfig,
}

impl StorageFactory {
    /// Create a new storage factory
    pub fn new(config: &StorageConfig) -> Self {
        Self {
            config: config.clone(),
        }
    }

    /// Create a storage adapter for a user
    pub async fn create_for_user(&self, user: &User) -> Result<Box<dyn StorageAdapter>, StorageError> {
        match user.storage.as_str() {
            "maildir" => {
                let maildir_path = if user.maildir.is_empty() {
                    format!("{}/{}", self.config.maildir_base, user.username)
                } else {
                    user.maildir.clone()
                };
                Ok(Box::new(MaildirAdapter::new(&maildir_path)?))
            }
            #[cfg(feature = "s3")]
            "s3" => {
                if user.s3_bucket.is_empty() {
                    return Err(StorageError::Config(
                        "S3 bucket not configured for user".to_string(),
                    ));
                }
                let adapter = S3Adapter::new(
                    &user.s3_bucket,
                    &user.s3_prefix,
                    &self.config.s3.region,
                    self.config.s3.endpoint.clone(),
                )
                .await?;
                Ok(Box::new(adapter))
            }
            #[cfg(not(feature = "s3"))]
            "s3" => Err(StorageError::Config(
                "S3 storage support not enabled. Compile with --features s3".to_string(),
            )),
            other => Err(StorageError::Config(format!(
                "Unknown storage type: {}",
                other
            ))),
        }
    }
}
