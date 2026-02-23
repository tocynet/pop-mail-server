//! Virtual host router

use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::sync::Arc;

use tokio::sync::RwLock;
use tracing::{info, warn};

use crate::config::Config;
use crate::vhost::domain::DomainConfig;

/// Virtual host router
pub struct VirtualHostRouter {
    domains: RwLock<HashMap<String, Arc<DomainConfig>>>,
    default_domain: Option<String>,
}

impl VirtualHostRouter {
    /// Create a new virtual host router from config
    pub fn new(_config: &Config) -> anyhow::Result<Self> {
        let mut domains = HashMap::new();

        // Load domain configurations from domains/ directory
        let domains_dir = Path::new("domains");
        if domains_dir.exists() && domains_dir.is_dir() {
            for entry in fs::read_dir(domains_dir)? {
                let entry = entry?;
                let path = entry.path();

                if path.extension().map_or(false, |e| e == "toml") {
                    match DomainConfig::load(&path) {
                        Ok(domain_config) => {
                            let domain_name = domain_config.domain.name.to_lowercase();
                            info!("Loaded domain configuration: {}", domain_name);
                            domains.insert(domain_name, Arc::new(domain_config));
                        }
                        Err(e) => {
                            warn!("Failed to load domain config {:?}: {}", path, e);
                        }
                    }
                }
            }
        }

        Ok(Self {
            domains: RwLock::new(domains),
            default_domain: None,
        })
    }

    /// Get domain configuration by name
    pub async fn get_domain(&self, name: &str) -> Option<Arc<DomainConfig>> {
        let domains = self.domains.read().await;
        domains.get(&name.to_lowercase()).cloned()
    }

    /// Add or update a domain
    pub async fn add_domain(&self, config: DomainConfig) {
        let name = config.domain.name.to_lowercase();
        self.domains.write().await.insert(name, Arc::new(config));
    }

    /// Remove a domain
    pub async fn remove_domain(&self, name: &str) -> Option<Arc<DomainConfig>> {
        self.domains.write().await.remove(&name.to_lowercase())
    }

    /// List all domains
    pub async fn list_domains(&self) -> Vec<String> {
        self.domains.read().await.keys().cloned().collect()
    }

    /// Check if a domain exists and is enabled
    pub async fn is_domain_enabled(&self, name: &str) -> bool {
        if let Some(domain) = self.get_domain(name).await {
            domain.domain.enabled
        } else {
            false
        }
    }

    /// Get the default domain name
    pub fn default_domain(&self) -> Option<&str> {
        self.default_domain.as_deref()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vhost::domain::{DomainInfo, DomainTlsConfig};

    #[tokio::test]
    async fn test_add_and_get_domain() {
        let config = Config::default();
        let router = VirtualHostRouter::new(&config).unwrap();

        let domain_config = DomainConfig {
            domain: DomainInfo {
                name: "example.com".to_string(),
                enabled: true,
                default_storage: "maildir".to_string(),
                maildir_base: "/var/mail/example.com".to_string(),
            },
            tls: DomainTlsConfig::default(),
        };

        router.add_domain(domain_config).await;

        let found = router.get_domain("example.com").await;
        assert!(found.is_some());
        assert_eq!(found.unwrap().domain.name, "example.com");
    }

    #[tokio::test]
    async fn test_case_insensitive() {
        let config = Config::default();
        let router = VirtualHostRouter::new(&config).unwrap();

        let domain_config = DomainConfig {
            domain: DomainInfo {
                name: "Example.COM".to_string(),
                enabled: true,
                default_storage: "maildir".to_string(),
                maildir_base: String::new(),
            },
            tls: DomainTlsConfig::default(),
        };

        router.add_domain(domain_config).await;

        // Should be found regardless of case
        assert!(router.get_domain("example.com").await.is_some());
        assert!(router.get_domain("EXAMPLE.COM").await.is_some());
        assert!(router.get_domain("Example.Com").await.is_some());
    }

    #[tokio::test]
    async fn test_remove_domain() {
        let config = Config::default();
        let router = VirtualHostRouter::new(&config).unwrap();

        let domain_config = DomainConfig {
            domain: DomainInfo {
                name: "example.com".to_string(),
                enabled: true,
                default_storage: "maildir".to_string(),
                maildir_base: String::new(),
            },
            tls: DomainTlsConfig::default(),
        };

        router.add_domain(domain_config).await;
        assert!(router.get_domain("example.com").await.is_some());

        router.remove_domain("example.com").await;
        assert!(router.get_domain("example.com").await.is_none());
    }
}
