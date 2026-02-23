//! TLS-enabled TCP listener for POP3 server

use std::fs::File;
use std::io::BufReader;
use std::net::SocketAddr;
use std::path::Path;
use std::sync::Arc;

use rustls::pki_types::CertificateDer;
use rustls::ServerConfig;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::Semaphore;
use tokio_rustls::TlsAcceptor;
use tracing::{error, info, warn};

use crate::auth::AuthStore;
use crate::config::Config;
use crate::security::RateLimiter;
use crate::server::Session;
use crate::storage::adapter::StorageFactory;
use crate::vhost::VirtualHostRouter;
use crate::webhook::WebhookDispatcher;

/// Main POP3 server
pub struct Pop3Server {
    config: Arc<Config>,
    tls_acceptor: TlsAcceptor,
    auth_store: Arc<AuthStore>,
    vhost_router: Arc<VirtualHostRouter>,
    storage_factory: Arc<StorageFactory>,
    webhook_dispatcher: Arc<WebhookDispatcher>,
    rate_limiter: Arc<RateLimiter>,
    connection_semaphore: Arc<Semaphore>,
}

impl Pop3Server {
    /// Create a new POP3 server with the given configuration
    pub async fn new(config: Config) -> anyhow::Result<Self> {
        let tls_config = load_tls_config(&config.tls.cert_path, &config.tls.key_path)?;
        let tls_acceptor = TlsAcceptor::from(Arc::new(tls_config));

        let auth_store = Arc::new(AuthStore::load("users.toml").await?);
        let vhost_router = Arc::new(VirtualHostRouter::new(&config)?);
        let storage_factory = Arc::new(StorageFactory::new(&config.storage));
        let webhook_dispatcher = Arc::new(WebhookDispatcher::new(&config.webhook));
        let rate_limiter = Arc::new(RateLimiter::new(config.security.rate_limit_per_second));
        let connection_semaphore = Arc::new(Semaphore::new(config.server.max_connections));

        Ok(Self {
            config: Arc::new(config),
            tls_acceptor,
            auth_store,
            vhost_router,
            storage_factory,
            webhook_dispatcher,
            rate_limiter,
            connection_semaphore,
        })
    }

    /// Run the POP3 server
    pub async fn run(&self) -> anyhow::Result<()> {
        let addr: SocketAddr = self.config.server.bind_address.parse()?;
        let listener = TcpListener::bind(addr).await?;

        info!("POP3 server listening on {} (TLS)", addr);

        loop {
            match listener.accept().await {
                Ok((stream, peer_addr)) => {
                    // Check rate limit
                    if !self.rate_limiter.check(peer_addr.ip()) {
                        warn!("Rate limit exceeded for {}", peer_addr);
                        continue;
                    }

                    // Acquire connection permit
                    let permit = match self.connection_semaphore.clone().try_acquire_owned() {
                        Ok(permit) => permit,
                        Err(_) => {
                            warn!("Max connections reached, rejecting {}", peer_addr);
                            continue;
                        }
                    };

                    let tls_acceptor = self.tls_acceptor.clone();
                    let config = self.config.clone();
                    let auth_store = self.auth_store.clone();
                    let vhost_router = self.vhost_router.clone();
                    let storage_factory = self.storage_factory.clone();
                    let webhook_dispatcher = self.webhook_dispatcher.clone();

                    tokio::spawn(async move {
                        let _permit = permit; // Hold permit until connection closes

                        if let Err(e) = handle_connection(
                            stream,
                            peer_addr,
                            tls_acceptor,
                            config,
                            auth_store,
                            vhost_router,
                            storage_factory,
                            webhook_dispatcher,
                        )
                        .await
                        {
                            error!("Connection error from {}: {}", peer_addr, e);
                        }
                    });
                }
                Err(e) => {
                    error!("Failed to accept connection: {}", e);
                }
            }
        }
    }
}

/// Handle a single client connection
async fn handle_connection(
    stream: TcpStream,
    peer_addr: SocketAddr,
    tls_acceptor: TlsAcceptor,
    config: Arc<Config>,
    auth_store: Arc<AuthStore>,
    vhost_router: Arc<VirtualHostRouter>,
    storage_factory: Arc<StorageFactory>,
    webhook_dispatcher: Arc<WebhookDispatcher>,
) -> anyhow::Result<()> {
    info!("New connection from {}", peer_addr);

    // Perform TLS handshake
    let tls_stream = tls_acceptor.accept(stream).await?;

    // Create and run session
    let mut session = Session::new(
        tls_stream,
        peer_addr,
        config,
        auth_store,
        vhost_router,
        storage_factory,
        webhook_dispatcher,
    );

    session.run().await?;

    info!("Connection closed from {}", peer_addr);
    Ok(())
}

/// Load TLS configuration from certificate and key files
fn load_tls_config(cert_path: &str, key_path: &str) -> anyhow::Result<ServerConfig> {
    let cert_file = File::open(Path::new(cert_path))?;
    let key_file = File::open(Path::new(key_path))?;

    let mut cert_reader = BufReader::new(cert_file);
    let mut key_reader = BufReader::new(key_file);

    let certs: Vec<CertificateDer<'static>> = rustls_pemfile::certs(&mut cert_reader)
        .collect::<Result<Vec<_>, _>>()?;

    let key = rustls_pemfile::private_key(&mut key_reader)?
        .ok_or_else(|| anyhow::anyhow!("No private key found in key file"))?;

    let config = ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(certs, key)?;

    Ok(config)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tls_config_missing_file() {
        let result = load_tls_config("nonexistent.crt", "nonexistent.key");
        assert!(result.is_err());
    }
}
