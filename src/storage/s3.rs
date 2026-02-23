//! S3 storage adapter
//!
//! This module is only available with the "s3" feature enabled.

#![cfg(feature = "s3")]

use async_trait::async_trait;
use aws_config::BehaviorVersion;
use aws_sdk_s3::Client;
use tracing::debug;

use crate::storage::adapter::{MessageInfo, StorageAdapter, StorageError};

/// S3 storage adapter
pub struct S3Adapter {
    client: Client,
    bucket: String,
    prefix: String,
}

impl S3Adapter {
    /// Create a new S3 adapter
    pub async fn new(
        bucket: &str,
        prefix: &str,
        region: &str,
        endpoint: Option<String>,
    ) -> Result<Self, StorageError> {
        let mut config_loader = aws_config::defaults(BehaviorVersion::latest())
            .region(aws_sdk_s3::config::Region::new(region.to_string()));

        // Use custom endpoint if provided (for MinIO, LocalStack, etc.)
        if let Some(endpoint_url) = endpoint {
            if !endpoint_url.is_empty() {
                config_loader = config_loader.endpoint_url(&endpoint_url);
            }
        }

        let config = config_loader.load().await;
        let client = Client::new(&config);

        Ok(Self {
            client,
            bucket: bucket.to_string(),
            prefix: prefix.to_string(),
        })
    }

    /// Get the full S3 key for a message
    fn message_key(&self, id: &str) -> String {
        if self.prefix.is_empty() {
            id.to_string()
        } else {
            format!("{}/{}", self.prefix.trim_end_matches('/'), id)
        }
    }

    /// Generate UIDL from S3 ETag or key
    fn generate_uidl(key: &str, etag: Option<&str>) -> String {
        // Use ETag if available, otherwise hash the key
        if let Some(etag) = etag {
            // Remove quotes from ETag
            etag.trim_matches('"').to_string()
        } else {
            let digest = md5::compute(key.as_bytes());
            hex::encode(digest.0)
        }
    }
}

#[async_trait]
impl StorageAdapter for S3Adapter {
    async fn list_messages(&self, _user: &str) -> Result<Vec<MessageInfo>, StorageError> {
        let mut messages = Vec::new();
        let prefix = if self.prefix.is_empty() {
            None
        } else {
            Some(self.prefix.clone())
        };

        let mut continuation_token: Option<String> = None;

        loop {
            let mut request = self.client.list_objects_v2().bucket(&self.bucket);

            if let Some(p) = &prefix {
                request = request.prefix(p);
            }

            if let Some(token) = &continuation_token {
                request = request.continuation_token(token);
            }

            let response = request
                .send()
                .await
                .map_err(|e| StorageError::Internal(e.to_string()))?;

            if let Some(contents) = response.contents {
                for object in contents {
                    if let Some(key) = object.key {
                        // Extract the message ID (filename without prefix)
                        let id = key
                            .strip_prefix(&format!("{}/", self.prefix.trim_end_matches('/')))
                            .unwrap_or(&key)
                            .to_string();

                        let size = object.size.unwrap_or(0) as usize;
                        let uidl = Self::generate_uidl(&key, object.e_tag.as_deref());

                        messages.push(MessageInfo { id, size, uidl });
                    }
                }
            }

            // Check if there are more objects
            if response.is_truncated.unwrap_or(false) {
                continuation_token = response.next_continuation_token;
            } else {
                break;
            }
        }

        // Sort by ID for consistent ordering
        messages.sort_by(|a, b| a.id.cmp(&b.id));

        debug!("Listed {} messages from S3", messages.len());
        Ok(messages)
    }

    async fn get_message(&self, _user: &str, id: &str) -> Result<Vec<u8>, StorageError> {
        let key = self.message_key(id);

        let response = self
            .client
            .get_object()
            .bucket(&self.bucket)
            .key(&key)
            .send()
            .await
            .map_err(|e| {
                if e.to_string().contains("NoSuchKey") {
                    StorageError::NotFound(id.to_string())
                } else {
                    StorageError::Internal(e.to_string())
                }
            })?;

        let content = response
            .body
            .collect()
            .await
            .map_err(|e| StorageError::Internal(e.to_string()))?
            .to_vec();

        debug!("Retrieved message {} from S3 ({} bytes)", id, content.len());
        Ok(content)
    }

    async fn delete_message(&self, _user: &str, id: &str) -> Result<(), StorageError> {
        let key = self.message_key(id);

        self.client
            .delete_object()
            .bucket(&self.bucket)
            .key(&key)
            .send()
            .await
            .map_err(|e| StorageError::Internal(e.to_string()))?;

        debug!("Deleted message {} from S3", id);
        Ok(())
    }

    async fn process_new_messages(&self, _user: &str) -> Result<(), StorageError> {
        // S3 doesn't have the concept of new/cur directories
        // Messages are immediately available
        Ok(())
    }
}
