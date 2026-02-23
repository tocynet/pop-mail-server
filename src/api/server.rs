//! REST API server

use std::net::SocketAddr;
use std::sync::Arc;

use tokio::net::TcpListener;
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::TraceLayer;
use tracing::info;

use crate::api::handlers::AppState;
use crate::api::routes::build_routes;
use crate::auth::AuthStore;
use crate::config::ApiConfig;
use crate::vhost::VirtualHostRouter;

/// REST API server
pub struct ApiServer {
    config: ApiConfig,
    auth_store: Arc<AuthStore>,
    vhost_router: Arc<VirtualHostRouter>,
}

impl ApiServer {
    /// Create a new API server
    pub fn new(
        config: ApiConfig,
        auth_store: Arc<AuthStore>,
        vhost_router: Arc<VirtualHostRouter>,
    ) -> Self {
        Self {
            config,
            auth_store,
            vhost_router,
        }
    }

    /// Run the API server
    pub async fn run(&self) -> anyhow::Result<()> {
        if !self.config.enabled {
            info!("API server disabled");
            return Ok(());
        }

        let state = Arc::new(AppState {
            auth_store: self.auth_store.clone(),
            vhost_router: self.vhost_router.clone(),
        });

        let cors = CorsLayer::new()
            .allow_origin(Any)
            .allow_methods(Any)
            .allow_headers(Any);

        let app = build_routes(state, &self.config.api_key)
            .layer(cors)
            .layer(TraceLayer::new_for_http());

        let addr: SocketAddr = self.config.bind_address.parse()?;
        let listener = TcpListener::bind(addr).await?;

        info!("API server listening on {}", addr);

        axum::serve(listener, app).await?;

        Ok(())
    }
}
