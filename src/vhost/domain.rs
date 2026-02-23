//! Domain configuration

use serde::{Deserialize, Serialize};
use std::path::Path;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum DomainError {
    #[error("Failed to read domain config: {0}")]
    ReadError(#[from] std::io::Error),
    #[error("Failed to parse domain config: {0}")]
    ParseError(#[from] toml::de::Error),
}

/// Domain configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DomainConfig {
    pub domain: DomainInfo,
    #[serde(default)]
    pub tls: DomainTlsConfig,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DomainInfo {
    pub name: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_storage")]
    pub default_storage: String,
    #[serde(default)]
    pub maildir_base: String,
}

fn default_true() -> bool {
    true
}

fn default_storage() -> String {
    "maildir".to_string()
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct DomainTlsConfig {
    #[serde(default)]
    pub cert_path: String,
    #[serde(default)]
    pub key_path: String,
}

impl DomainConfig {
    /// Load domain configuration from a TOML file
    pub fn load<P: AsRef<Path>>(path: P) -> Result<Self, DomainError> {
        let content = std::fs::read_to_string(path)?;
        let config: DomainConfig = toml::from_str(&content)?;
        Ok(config)
    }

    /// Save domain configuration to a TOML file
    pub fn save<P: AsRef<Path>>(&self, path: P) -> Result<(), DomainError> {
        let content = toml::to_string_pretty(self)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string()))?;
        std::fs::write(path, content)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_domain_config() {
        let toml_str = r#"
[domain]
name = "example.com"
enabled = true
default_storage = "maildir"
maildir_base = "/var/mail/example.com"

[tls]
cert_path = "certs/example.com.crt"
key_path = "certs/example.com.key"
"#;

        let config: DomainConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(config.domain.name, "example.com");
        assert!(config.domain.enabled);
        assert_eq!(config.domain.default_storage, "maildir");
    }
}
