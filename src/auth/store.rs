//! User authentication store

use std::collections::HashMap;
use std::path::Path;

use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::sync::RwLock;

use crate::auth::password::PasswordHasher;

#[derive(Error, Debug)]
pub enum AuthError {
    #[error("Failed to read users file: {0}")]
    ReadError(#[from] std::io::Error),
    #[error("Failed to parse users file: {0}")]
    ParseError(#[from] toml::de::Error),
    #[error("User not found")]
    UserNotFound,
    #[error("Invalid password")]
    InvalidPassword,
    #[error("User disabled")]
    UserDisabled,
    #[error("Password verification error: {0}")]
    PasswordError(String),
}

/// IMAP forwarding configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ImapForwardConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub host: String,
    #[serde(default = "default_imap_port")]
    pub port: u16,
    #[serde(default = "default_true")]
    pub tls: bool,
    #[serde(default)]
    pub username: String,
    #[serde(default)]
    pub password_env: String,
    #[serde(default = "default_folder")]
    pub target_folder: String,
    #[serde(default)]
    pub delete_after_forward: bool,
}

fn default_imap_port() -> u16 {
    993
}

fn default_true() -> bool {
    true
}

fn default_folder() -> String {
    "INBOX".to_string()
}

impl Default for ImapForwardConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            host: String::new(),
            port: 993,
            tls: true,
            username: String::new(),
            password_env: String::new(),
            target_folder: "INBOX".to_string(),
            delete_after_forward: false,
        }
    }
}

/// User configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct User {
    pub username: String,
    #[serde(default)]
    pub domain: String,
    pub password_hash: String,
    #[serde(default)]
    pub maildir: String,
    #[serde(default = "default_storage")]
    pub storage: String,
    #[serde(default)]
    pub s3_bucket: String,
    #[serde(default)]
    pub s3_prefix: String,
    #[serde(default)]
    pub webhook_url: String,
    #[serde(default)]
    pub webhook_events: Vec<String>,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub imap_forward: ImapForwardConfig,
}

fn default_storage() -> String {
    "maildir".to_string()
}

/// Users file structure
#[derive(Debug, Deserialize, Serialize)]
struct UsersFile {
    users: Vec<User>,
}

/// Authentication store
pub struct AuthStore {
    users: RwLock<HashMap<String, User>>,
    hasher: PasswordHasher,
}

impl AuthStore {
    /// Create a new empty auth store
    pub fn new() -> Self {
        Self {
            users: RwLock::new(HashMap::new()),
            hasher: PasswordHasher::new(),
        }
    }

    /// Load users from a TOML file
    pub async fn load<P: AsRef<Path>>(path: P) -> Result<Self, AuthError> {
        let content = std::fs::read_to_string(path)?;
        let users_file: UsersFile = toml::from_str(&content)?;

        let mut users_map = HashMap::new();
        for user in users_file.users {
            let key = Self::make_key(&user.username, &user.domain);
            users_map.insert(key, user);
        }

        Ok(Self {
            users: RwLock::new(users_map),
            hasher: PasswordHasher::new(),
        })
    }

    /// Make a lookup key from username and domain
    fn make_key(username: &str, domain: &str) -> String {
        if domain.is_empty() {
            username.to_lowercase()
        } else {
            format!("{}@{}", username.to_lowercase(), domain.to_lowercase())
        }
    }

    /// Parse a username that may include a domain
    fn parse_username(full_username: &str) -> (String, String) {
        if let Some(at_pos) = full_username.rfind('@') {
            let username = full_username[..at_pos].to_string();
            let domain = full_username[at_pos + 1..].to_string();
            (username, domain)
        } else {
            (full_username.to_string(), String::new())
        }
    }

    /// Authenticate a user
    pub async fn authenticate(&self, username: &str, password: &str) -> Result<User, AuthError> {
        let (user_part, domain_part) = Self::parse_username(username);
        let key = Self::make_key(&user_part, &domain_part);

        let users = self.users.read().await;
        let user = users.get(&key).ok_or(AuthError::UserNotFound)?;

        if !user.enabled {
            return Err(AuthError::UserDisabled);
        }

        let is_valid = self
            .hasher
            .verify(password, &user.password_hash)
            .map_err(|e| AuthError::PasswordError(e.to_string()))?;

        if is_valid {
            Ok(user.clone())
        } else {
            Err(AuthError::InvalidPassword)
        }
    }

    /// Get a user by username (without authentication)
    pub async fn get_user(&self, username: &str) -> Option<User> {
        let (user_part, domain_part) = Self::parse_username(username);
        let key = Self::make_key(&user_part, &domain_part);
        self.users.read().await.get(&key).cloned()
    }

    /// Add or update a user
    pub async fn upsert_user(&self, user: User) {
        let key = Self::make_key(&user.username, &user.domain);
        self.users.write().await.insert(key, user);
    }

    /// Remove a user
    pub async fn remove_user(&self, username: &str, domain: &str) -> Option<User> {
        let key = Self::make_key(username, domain);
        self.users.write().await.remove(&key)
    }

    /// List all users
    pub async fn list_users(&self) -> Vec<User> {
        self.users.read().await.values().cloned().collect()
    }

    /// List users for a specific domain
    pub async fn list_users_by_domain(&self, domain: &str) -> Vec<User> {
        let domain_lower = domain.to_lowercase();
        self.users
            .read()
            .await
            .values()
            .filter(|u| u.domain.to_lowercase() == domain_lower)
            .cloned()
            .collect()
    }

    /// Save users to a TOML file
    pub async fn save<P: AsRef<Path>>(&self, path: P) -> Result<(), AuthError> {
        let users: Vec<User> = self.users.read().await.values().cloned().collect();
        let users_file = UsersFile { users };
        let content = toml::to_string_pretty(&users_file)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string()))?;
        std::fs::write(path, content)?;
        Ok(())
    }

    /// Hash a password (for creating new users)
    pub fn hash_password(&self, password: &str) -> Result<String, AuthError> {
        self.hasher
            .hash(password)
            .map_err(|e| AuthError::PasswordError(e.to_string()))
    }
}

impl Default for AuthStore {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_make_key() {
        assert_eq!(AuthStore::make_key("alice", ""), "alice");
        assert_eq!(
            AuthStore::make_key("alice", "example.com"),
            "alice@example.com"
        );
        assert_eq!(
            AuthStore::make_key("Alice", "Example.COM"),
            "alice@example.com"
        );
    }

    #[tokio::test]
    async fn test_parse_username() {
        assert_eq!(
            AuthStore::parse_username("alice"),
            ("alice".to_string(), String::new())
        );
        assert_eq!(
            AuthStore::parse_username("alice@example.com"),
            ("alice".to_string(), "example.com".to_string())
        );
    }

    #[tokio::test]
    async fn test_upsert_and_get() {
        let store = AuthStore::new();
        let hasher = PasswordHasher::new();
        let hash = hasher.hash("secret").unwrap();

        let user = User {
            username: "alice".to_string(),
            domain: "example.com".to_string(),
            password_hash: hash,
            maildir: "/var/mail/alice".to_string(),
            storage: "maildir".to_string(),
            s3_bucket: String::new(),
            s3_prefix: String::new(),
            webhook_url: String::new(),
            webhook_events: vec![],
            enabled: true,
            imap_forward: ImapForwardConfig::default(),
        };

        store.upsert_user(user.clone()).await;

        let found = store.get_user("alice@example.com").await;
        assert!(found.is_some());
        assert_eq!(found.unwrap().username, "alice");
    }

    #[tokio::test]
    async fn test_authenticate() {
        let store = AuthStore::new();
        let hash = store.hash_password("secret").unwrap();

        let user = User {
            username: "alice".to_string(),
            domain: "example.com".to_string(),
            password_hash: hash,
            maildir: "/var/mail/alice".to_string(),
            storage: "maildir".to_string(),
            s3_bucket: String::new(),
            s3_prefix: String::new(),
            webhook_url: String::new(),
            webhook_events: vec![],
            enabled: true,
            imap_forward: ImapForwardConfig::default(),
        };

        store.upsert_user(user).await;

        // Valid authentication
        let result = store.authenticate("alice@example.com", "secret").await;
        assert!(result.is_ok());

        // Invalid password
        let result = store.authenticate("alice@example.com", "wrong").await;
        assert!(matches!(result, Err(AuthError::InvalidPassword)));

        // Unknown user
        let result = store.authenticate("bob@example.com", "secret").await;
        assert!(matches!(result, Err(AuthError::UserNotFound)));
    }
}
