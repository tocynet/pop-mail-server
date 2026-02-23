//! Virtual host support for multiple domains

pub mod domain;
pub mod router;

pub use domain::DomainConfig;
pub use router::VirtualHostRouter;
