//! IMAP forwarder for transferring messages to IMAP servers

use std::env;

use async_native_tls::TlsConnector;
use thiserror::Error;
use tracing::{debug, info};

use crate::auth::ImapForwardConfig;

#[derive(Error, Debug)]
pub enum ForwardError {
    #[error("Connection failed: {0}")]
    Connection(String),
    #[error("Authentication failed: {0}")]
    Auth(String),
    #[error("IMAP error: {0}")]
    Imap(String),
    #[error("Configuration error: {0}")]
    Config(String),
}

/// IMAP forwarder for sending messages to an IMAP server
pub struct ImapForwarder {
    config: ImapForwardConfig,
}

impl ImapForwarder {
    /// Create a new IMAP forwarder
    pub fn new(config: ImapForwardConfig) -> Self {
        Self { config }
    }

    /// Check if forwarding is enabled
    pub fn is_enabled(&self) -> bool {
        self.config.enabled
    }

    /// Get the password from environment variable
    fn get_password(&self) -> Result<String, ForwardError> {
        if self.config.password_env.is_empty() {
            return Err(ForwardError::Config(
                "Password environment variable not configured".to_string(),
            ));
        }

        env::var(&self.config.password_env).map_err(|_| {
            ForwardError::Config(format!(
                "Environment variable {} not set",
                self.config.password_env
            ))
        })
    }

    /// Forward a message to the IMAP server
    pub async fn forward(&self, message: &[u8]) -> Result<(), ForwardError> {
        if !self.config.enabled {
            return Ok(());
        }

        let password = self.get_password()?;

        info!(
            "Forwarding message to {}:{}/{}",
            self.config.host, self.config.port, self.config.target_folder
        );

        if self.config.tls {
            self.forward_tls(message, &password).await
        } else {
            self.forward_plain(message, &password).await
        }
    }

    /// Forward using TLS connection
    async fn forward_tls(&self, message: &[u8], password: &str) -> Result<(), ForwardError> {
        use async_std::net::TcpStream;
        use async_imap::Client as ImapClient;

        let addr = format!("{}:{}", self.config.host, self.config.port);

        // Connect to the server using async_std (compatible with async-imap)
        let tcp_stream = TcpStream::connect(&addr)
            .await
            .map_err(|e| ForwardError::Connection(e.to_string()))?;

        // Establish TLS connection
        let tls = TlsConnector::new();
        let tls_stream = tls
            .connect(&self.config.host, tcp_stream)
            .await
            .map_err(|e| ForwardError::Connection(e.to_string()))?;

        // Create IMAP client
        let client = ImapClient::new(tls_stream);

        // Login
        debug!("Logging in as {}", self.config.username);
        let mut session = client
            .login(&self.config.username, password)
            .await
            .map_err(|(e, _)| ForwardError::Auth(e.to_string()))?;

        // Append message to target folder
        debug!("Appending message to {}", self.config.target_folder);
        session
            .append(&self.config.target_folder, None, None, message)
            .await
            .map_err(|e| ForwardError::Imap(e.to_string()))?;

        // Logout
        session
            .logout()
            .await
            .map_err(|e| ForwardError::Imap(e.to_string()))?;

        info!("Message forwarded successfully");
        Ok(())
    }

    /// Forward using plain connection (not recommended)
    async fn forward_plain(&self, message: &[u8], password: &str) -> Result<(), ForwardError> {
        use async_std::net::TcpStream;
        use async_imap::Client as ImapClient;

        let addr = format!("{}:{}", self.config.host, self.config.port);

        // Connect to the server using async_std (compatible with async-imap)
        let tcp_stream = TcpStream::connect(&addr)
            .await
            .map_err(|e| ForwardError::Connection(e.to_string()))?;

        // Create IMAP client
        let client = ImapClient::new(tcp_stream);

        // Login
        debug!("Logging in as {}", self.config.username);
        let mut session = client
            .login(&self.config.username, password)
            .await
            .map_err(|(e, _)| ForwardError::Auth(e.to_string()))?;

        // Append message to target folder
        debug!("Appending message to {}", self.config.target_folder);
        session
            .append(&self.config.target_folder, None, None, message)
            .await
            .map_err(|e| ForwardError::Imap(e.to_string()))?;

        // Logout
        session
            .logout()
            .await
            .map_err(|e| ForwardError::Imap(e.to_string()))?;

        info!("Message forwarded successfully");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_forwarder_disabled() {
        let config = ImapForwardConfig::default();
        let forwarder = ImapForwarder::new(config);
        assert!(!forwarder.is_enabled());
    }

    #[test]
    fn test_forwarder_enabled() {
        let config = ImapForwardConfig {
            enabled: true,
            host: "imap.example.com".to_string(),
            port: 993,
            tls: true,
            username: "user@example.com".to_string(),
            password_env: "IMAP_PASSWORD".to_string(),
            target_folder: "INBOX".to_string(),
            delete_after_forward: false,
        };
        let forwarder = ImapForwarder::new(config);
        assert!(forwarder.is_enabled());
    }
}
