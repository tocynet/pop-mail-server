//! IMAP forwarding functionality

pub mod forwarder;
pub mod watcher;

pub use forwarder::ImapForwarder;
pub use watcher::MailWatcher;
