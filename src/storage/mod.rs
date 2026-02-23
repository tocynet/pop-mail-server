//! Storage adapters for mail storage

pub mod adapter;
pub mod maildir;

#[cfg(feature = "s3")]
pub mod s3;

pub use adapter::{MessageInfo, StorageAdapter};
pub use self::maildir::MaildirAdapter;

#[cfg(feature = "s3")]
pub use self::s3::S3Adapter;
