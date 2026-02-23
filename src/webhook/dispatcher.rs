//! Webhook dispatcher

use std::net::SocketAddr;
use std::time::Duration;

use chrono::Utc;
use reqwest::Client;
use serde::Serialize;
use tracing::{debug, error, warn};

use crate::config::WebhookConfig;

/// Webhook events
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum WebhookEvent {
    SessionStart {
        username: String,
        #[serde(serialize_with = "serialize_addr")]
        peer_addr: SocketAddr,
    },
    SessionEnd {
        username: String,
        #[serde(serialize_with = "serialize_addr")]
        peer_addr: SocketAddr,
    },
    Command {
        username: String,
        command: String,
        #[serde(serialize_with = "serialize_addr")]
        peer_addr: SocketAddr,
    },
}

fn serialize_addr<S>(addr: &SocketAddr, serializer: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    serializer.serialize_str(&addr.to_string())
}

/// Webhook payload
#[derive(Debug, Serialize)]
struct WebhookPayload {
    timestamp: String,
    #[serde(flatten)]
    event: WebhookEvent,
}

/// Webhook dispatcher for sending notifications
pub struct WebhookDispatcher {
    client: Client,
    timeout: Duration,
    retry_count: u32,
}

impl WebhookDispatcher {
    /// Create a new webhook dispatcher
    pub fn new(config: &WebhookConfig) -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(config.timeout_secs))
            .build()
            .unwrap_or_default();

        Self {
            client,
            timeout: Duration::from_secs(config.timeout_secs),
            retry_count: config.retry_count,
        }
    }

    /// Dispatch a webhook event
    pub async fn dispatch(&self, event: WebhookEvent) {
        // Get webhook URL from event
        let _username = match &event {
            WebhookEvent::SessionStart { username, .. } => username,
            WebhookEvent::SessionEnd { username, .. } => username,
            WebhookEvent::Command { username, .. } => username,
        };

        // Note: In a real implementation, you would look up the user's webhook URL
        // For now, we just log the event
        debug!("Webhook event: {:?}", event);
    }

    /// Send a webhook to a specific URL
    pub async fn send_to_url(&self, url: &str, event: WebhookEvent) -> bool {
        if url.is_empty() {
            return true;
        }

        let payload = WebhookPayload {
            timestamp: Utc::now().to_rfc3339(),
            event,
        };

        for attempt in 0..=self.retry_count {
            if attempt > 0 {
                // Exponential backoff
                let delay = Duration::from_millis(100 * (2_u64.pow(attempt)));
                tokio::time::sleep(delay).await;
                debug!("Webhook retry {} for {}", attempt, url);
            }

            match self
                .client
                .post(url)
                .json(&payload)
                .timeout(self.timeout)
                .send()
                .await
            {
                Ok(response) => {
                    if response.status().is_success() {
                        debug!("Webhook sent successfully to {}", url);
                        return true;
                    } else {
                        warn!(
                            "Webhook failed with status {} for {}",
                            response.status(),
                            url
                        );
                    }
                }
                Err(e) => {
                    error!("Webhook error for {}: {}", url, e);
                }
            }
        }

        error!("Webhook failed after {} retries for {}", self.retry_count, url);
        false
    }
}

impl Default for WebhookDispatcher {
    fn default() -> Self {
        Self::new(&WebhookConfig {
            timeout_secs: 10,
            retry_count: 3,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_webhook_event_serialization() {
        let event = WebhookEvent::SessionStart {
            username: "alice".to_string(),
            peer_addr: "127.0.0.1:12345".parse().unwrap(),
        };

        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("session_start"));
        assert!(json.contains("alice"));
    }

    #[test]
    fn test_webhook_payload() {
        let event = WebhookEvent::Command {
            username: "alice".to_string(),
            command: "retr".to_string(),
            peer_addr: "127.0.0.1:12345".parse().unwrap(),
        };

        let payload = WebhookPayload {
            timestamp: "2024-01-01T00:00:00Z".to_string(),
            event,
        };

        let json = serde_json::to_string(&payload).unwrap();
        assert!(json.contains("timestamp"));
        assert!(json.contains("command"));
        assert!(json.contains("retr"));
    }
}
