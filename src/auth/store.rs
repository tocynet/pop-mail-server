//! User authentication store

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use notify::{Config as NotifyConfig, RecommendedWatcher, RecursiveMode, Watcher};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::sync::{mpsc, RwLock};
use tracing::{debug, error, info, warn};

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

/// Duration to ignore file changes after saving (to prevent self-triggered reload)
const SAVE_DEBOUNCE_DURATION: Duration = Duration::from_millis(500);

/// Authentication store with auto-save and hot-reload support
pub struct AuthStore {
    users: RwLock<HashMap<String, User>>,
    hasher: PasswordHasher,
    /// Path to the users file (for auto-save and hot-reload)
    file_path: Option<PathBuf>,
    /// Timestamp of last save (for self-trigger prevention)
    last_save_time: RwLock<Option<Instant>>,
    /// Flag to temporarily disable auto-save (during reload)
    save_disabled: AtomicBool,
}

impl AuthStore {
    /// Create a new empty auth store
    pub fn new() -> Self {
        Self {
            users: RwLock::new(HashMap::new()),
            hasher: PasswordHasher::new(),
            file_path: None,
            last_save_time: RwLock::new(None),
            save_disabled: AtomicBool::new(false),
        }
    }

    /// Load users from a TOML file
    pub async fn load<P: AsRef<Path>>(path: P) -> Result<Self, AuthError> {
        let path_buf = path.as_ref().to_path_buf();
        let content = std::fs::read_to_string(&path_buf)?;
        let users_file: UsersFile = toml::from_str(&content)?;

        let mut users_map = HashMap::new();
        for user in users_file.users {
            let key = Self::make_key(&user.username, &user.domain);
            users_map.insert(key, user);
        }

        info!("Loaded {} users from {:?}", users_map.len(), path_buf);

        Ok(Self {
            users: RwLock::new(users_map),
            hasher: PasswordHasher::new(),
            file_path: Some(path_buf),
            last_save_time: RwLock::new(None),
            save_disabled: AtomicBool::new(false),
        })
    }

    /// Reload users from the file (for hot-reload)
    pub async fn reload(&self) -> Result<(), AuthError> {
        let path = match &self.file_path {
            Some(p) => p.clone(),
            None => {
                warn!("Cannot reload: no file path configured");
                return Ok(());
            }
        };

        // Check if this is a self-triggered reload (we just saved)
        {
            let last_save = self.last_save_time.read().await;
            if let Some(save_time) = *last_save {
                if save_time.elapsed() < SAVE_DEBOUNCE_DURATION {
                    debug!("Ignoring reload triggered by our own save");
                    return Ok(());
                }
            }
        }

        info!("Reloading users from {:?}", path);

        let content = std::fs::read_to_string(&path)?;
        let users_file: UsersFile = toml::from_str(&content)?;

        let mut users_map = HashMap::new();
        for user in users_file.users {
            let key = Self::make_key(&user.username, &user.domain);
            users_map.insert(key, user);
        }

        // Replace users atomically
        {
            let mut users = self.users.write().await;
            *users = users_map;
        }

        info!("Reloaded {} users", self.users.read().await.len());
        Ok(())
    }

    /// Auto-save to file if file_path is configured
    async fn auto_save(&self) -> Result<(), AuthError> {
        if self.save_disabled.load(Ordering::SeqCst) {
            return Ok(());
        }

        if let Some(path) = &self.file_path {
            self.save(path).await?;
            // Update last save time for self-trigger prevention
            let mut last_save = self.last_save_time.write().await;
            *last_save = Some(Instant::now());
            debug!("Auto-saved users to {:?}", path);
        }
        Ok(())
    }

    /// Start file watcher for hot-reload
    /// Returns a channel sender that can be used to stop the watcher
    pub fn start_watcher(
        self: &std::sync::Arc<Self>,
    ) -> Result<mpsc::Sender<()>, AuthError> {
        let path = match &self.file_path {
            Some(p) => p.clone(),
            None => {
                return Err(AuthError::ReadError(std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    "No file path configured for watching",
                )));
            }
        };

        let (stop_tx, mut stop_rx) = mpsc::channel::<()>(1);
        let (event_tx, mut event_rx) = mpsc::channel::<()>(16);

        // Create file watcher
        let event_tx_clone = event_tx.clone();
        let path_clone = path.clone();
        let mut watcher = RecommendedWatcher::new(
            move |res: Result<notify::Event, notify::Error>| {
                match res {
                    Ok(event) => {
                        if event.kind.is_modify() || event.kind.is_create() {
                            debug!("File change detected: {:?}", event);
                            let _ = event_tx_clone.try_send(());
                        }
                    }
                    Err(e) => {
                        error!("File watcher error: {}", e);
                    }
                }
            },
            NotifyConfig::default(),
        )
        .map_err(|e| {
            AuthError::ReadError(std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("Failed to create file watcher: {}", e),
            ))
        })?;

        watcher
            .watch(&path, RecursiveMode::NonRecursive)
            .map_err(|e| {
                AuthError::ReadError(std::io::Error::new(
                    std::io::ErrorKind::Other,
                    format!("Failed to watch file: {}", e),
                ))
            })?;

        info!("Started file watcher for {:?}", path);

        // Spawn watcher task
        let store = std::sync::Arc::clone(self);
        tokio::spawn(async move {
            // Keep watcher alive
            let _watcher = watcher;

            loop {
                tokio::select! {
                    _ = stop_rx.recv() => {
                        info!("Stopping file watcher for {:?}", path_clone);
                        break;
                    }
                    Some(_) = event_rx.recv() => {
                        // Debounce: wait a bit for multiple rapid changes
                        tokio::time::sleep(Duration::from_millis(100)).await;
                        // Drain any additional events
                        while event_rx.try_recv().is_ok() {}

                        if let Err(e) = store.reload().await {
                            error!("Failed to reload users: {}", e);
                        }
                    }
                }
            }
        });

        Ok(stop_tx)
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

    /// Add or update a user (auto-saves to file)
    pub async fn upsert_user(&self, user: User) {
        let key = Self::make_key(&user.username, &user.domain);
        self.users.write().await.insert(key, user);

        // Auto-save to file
        if let Err(e) = self.auto_save().await {
            error!("Failed to auto-save after upsert: {}", e);
        }
    }

    /// Remove a user (auto-saves to file)
    pub async fn remove_user(&self, username: &str, domain: &str) -> Option<User> {
        let key = Self::make_key(username, domain);
        let removed = self.users.write().await.remove(&key);

        // Auto-save to file if user was removed
        if removed.is_some() {
            if let Err(e) = self.auto_save().await {
                error!("Failed to auto-save after remove: {}", e);
            }
        }

        removed
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
        let mut users: Vec<User> = self.users.read().await.values().cloned().collect();
        // Sort for consistent output
        users.sort_by(|a, b| {
            (&a.domain, &a.username).cmp(&(&b.domain, &b.username))
        });
        let users_file = UsersFile { users };

        // Add header comment
        let mut content = String::from(
            "# User Authentication Configuration\n\
             # Password hashes are generated using Argon2id\n\
             # Use `pop3ctl hash-password` to generate new hashes\n\
             # This file is auto-managed by the server - manual edits will be hot-reloaded\n\n",
        );
        content.push_str(
            &toml::to_string_pretty(&users_file)
                .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string()))?,
        );

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

/// Re-export for convenience
pub use notify;

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
