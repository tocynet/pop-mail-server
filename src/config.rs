//! Configuration loading and management

use serde::{Deserialize, Serialize};
use std::path::Path;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ConfigError {
    #[error("Failed to read config file: {0}")]
    ReadError(#[from] std::io::Error),
    #[error("Failed to parse config: {0}")]
    ParseError(#[from] toml::de::Error),
    #[error("Invalid configuration: {0}")]
    ValidationError(String),
}

/// Main server configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Config {
    pub server: ServerConfig,
    pub tls: TlsConfig,
    pub security: SecurityConfig,
    pub storage: StorageConfig,
    pub webhook: WebhookConfig,
    pub api: ApiConfig,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ServerConfig {
    pub bind_address: String,
    #[serde(default = "default_max_connections")]
    pub max_connections: usize,
}

fn default_max_connections() -> usize {
    100
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TlsConfig {
    pub cert_path: String,
    pub key_path: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SecurityConfig {
    #[serde(default = "default_command_timeout")]
    pub command_timeout_secs: u64,
    #[serde(default = "default_idle_timeout")]
    pub idle_timeout_secs: u64,
    #[serde(default = "default_max_command_length")]
    pub max_command_length: usize,
    #[serde(default = "default_rate_limit")]
    pub rate_limit_per_second: u32,
    #[serde(default = "default_max_auth_attempts")]
    pub max_auth_attempts: u32,
}

fn default_command_timeout() -> u64 {
    30
}

fn default_idle_timeout() -> u64 {
    300
}

fn default_max_command_length() -> usize {
    1000
}

fn default_rate_limit() -> u32 {
    10
}

fn default_max_auth_attempts() -> u32 {
    5
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct StorageConfig {
    #[serde(default = "default_storage_type")]
    pub default_type: String,
    pub maildir_base: String,
    #[serde(default)]
    pub s3: S3Config,
}

fn default_storage_type() -> String {
    "maildir".to_string()
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct S3Config {
    #[serde(default = "default_region")]
    pub region: String,
    #[serde(default)]
    pub endpoint: String,
}

fn default_region() -> String {
    "us-east-1".to_string()
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct WebhookConfig {
    #[serde(default = "default_webhook_timeout")]
    pub timeout_secs: u64,
    #[serde(default = "default_retry_count")]
    pub retry_count: u32,
}

fn default_webhook_timeout() -> u64 {
    10
}

fn default_retry_count() -> u32 {
    3
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ApiConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_api_bind")]
    pub bind_address: String,
    #[serde(default)]
    pub api_key: String,
}

fn default_api_bind() -> String {
    "127.0.0.1:8080".to_string()
}

impl Config {
    /// Load configuration from a TOML file
    pub fn load<P: AsRef<Path>>(path: P) -> Result<Self, ConfigError> {
        let content = std::fs::read_to_string(path)?;
        let config: Config = toml::from_str(&content)?;
        config.validate()?;
        Ok(config)
    }

    /// Validate configuration values
    fn validate(&self) -> Result<(), ConfigError> {
        if self.server.max_connections == 0 {
            return Err(ConfigError::ValidationError(
                "max_connections must be greater than 0".to_string(),
            ));
        }

        if self.security.rate_limit_per_second == 0 {
            return Err(ConfigError::ValidationError(
                "rate_limit_per_second must be greater than 0".to_string(),
            ));
        }

        Ok(())
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            server: ServerConfig {
                bind_address: "0.0.0.0:995".to_string(),
                max_connections: 100,
            },
            tls: TlsConfig {
                cert_path: "certs/server.crt".to_string(),
                key_path: "certs/server.key".to_string(),
            },
            security: SecurityConfig {
                command_timeout_secs: 30,
                idle_timeout_secs: 300,
                max_command_length: 1000,
                rate_limit_per_second: 10,
                max_auth_attempts: 5,
            },
            storage: StorageConfig {
                default_type: "maildir".to_string(),
                maildir_base: "/var/mail".to_string(),
                s3: S3Config::default(),
            },
            webhook: WebhookConfig {
                timeout_secs: 10,
                retry_count: 3,
            },
            api: ApiConfig {
                enabled: true,
                bind_address: "127.0.0.1:8080".to_string(),
                api_key: String::new(),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = Config::default();
        assert_eq!(config.server.bind_address, "0.0.0.0:995");
        assert_eq!(config.server.max_connections, 100);
    }
}
