//! POP3 Mail Server Library
//!
//! A secure POP3 mail server implementation in Rust with support for:
//! - TLS encryption (required)
//! - Maildir and S3 storage backends
//! - Virtual hosting (multiple domains)
//! - Webhook notifications
//! - IMAP forwarding
//! - REST API for management

pub mod config;
pub mod server;
pub mod protocol;
pub mod vhost;
pub mod auth;
pub mod storage;
pub mod webhook;
pub mod imap_forward;
pub mod api;
pub mod security;

pub use config::Config;

/// Server version
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Default POP3S port
pub const DEFAULT_POP3S_PORT: u16 = 995;
