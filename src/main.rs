//! POP3 Mail Server
//!
//! A secure POP3 mail server implementation in Rust.

use std::sync::Arc;

use clap::Parser;
use tracing::{info, Level};
use tracing_subscriber::FmtSubscriber;

use pop_mail_server::api::ApiServer;
use pop_mail_server::auth::AuthStore;
use pop_mail_server::config::Config;
use pop_mail_server::server::Pop3Server;
use pop_mail_server::vhost::VirtualHostRouter;

#[derive(Parser)]
#[command(name = "pop3-server")]
#[command(about = "Secure POP3 Mail Server", long_about = None)]
struct Cli {
    /// Configuration file path
    #[arg(short, long, default_value = "config.toml")]
    config: String,

    /// Users file path
    #[arg(short, long, default_value = "users.toml")]
    users: String,

    /// Log level (trace, debug, info, warn, error)
    #[arg(short, long, default_value = "info")]
    log_level: String,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    // Initialize logging
    let log_level = match cli.log_level.to_lowercase().as_str() {
        "trace" => Level::TRACE,
        "debug" => Level::DEBUG,
        "info" => Level::INFO,
        "warn" => Level::WARN,
        "error" => Level::ERROR,
        _ => Level::INFO,
    };

    let subscriber = FmtSubscriber::builder()
        .with_max_level(log_level)
        .with_target(true)
        .with_thread_ids(true)
        .with_file(true)
        .with_line_number(true)
        .finish();

    tracing::subscriber::set_global_default(subscriber)?;

    info!("Starting POP3 Server v{}", pop_mail_server::VERSION);

    // Load configuration
    info!("Loading configuration from {}", cli.config);
    let config = Config::load(&cli.config)?;

    // Load auth store
    info!("Loading users from {}", cli.users);
    let auth_store = Arc::new(AuthStore::load(&cli.users).await?);

    // Start file watcher for hot-reload
    let _watcher_stop = match auth_store.start_watcher() {
        Ok(stop_tx) => {
            info!("Users file watcher started - changes will be hot-reloaded");
            Some(stop_tx)
        }
        Err(e) => {
            tracing::warn!("Failed to start users file watcher: {}", e);
            None
        }
    };

    // Create virtual host router
    let vhost_router = Arc::new(VirtualHostRouter::new(&config)?);

    // Start API server if enabled
    if config.api.enabled {
        let api_server = ApiServer::new(
            config.api.clone(),
            auth_store.clone(),
            vhost_router.clone(),
        );

        tokio::spawn(async move {
            if let Err(e) = api_server.run().await {
                tracing::error!("API server error: {}", e);
            }
        });
    }

    // Start POP3 server
    info!(
        "Starting POP3 server on {} with TLS",
        config.server.bind_address
    );
    let server = Pop3Server::new(config).await?;
    server.run().await?;

    Ok(())
}
