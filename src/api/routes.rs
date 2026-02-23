//! API route definitions

use std::sync::Arc;

use axum::{
    extract::Request,
    http::StatusCode,
    middleware::{self, Next},
    response::Response,
    routing::{delete, get, post, put},
    Router,
};

use crate::api::handlers::{self, AppState};

/// API key authentication middleware
async fn auth_middleware(
    req: Request,
    next: Next,
    api_key: String,
) -> Result<Response, StatusCode> {
    // Skip auth for health check
    if req.uri().path() == "/api/v1/health" {
        return Ok(next.run(req).await);
    }

    // Check API key
    if let Some(key) = req.headers().get("x-api-key") {
        if key.to_str().unwrap_or("") == api_key {
            return Ok(next.run(req).await);
        }
    }

    Err(StatusCode::UNAUTHORIZED)
}

/// Build the API routes
pub fn build_routes(state: Arc<AppState>, api_key: &str) -> Router {
    let api_key = api_key.to_string();

    Router::new()
        // Health check (no auth required)
        .route("/api/v1/health", get(handlers::health))
        // Domain routes
        .route("/api/v1/domains", get(handlers::list_domains))
        .route("/api/v1/domains", post(handlers::create_domain))
        .route("/api/v1/domains/:domain", get(handlers::get_domain))
        .route("/api/v1/domains/:domain", delete(handlers::delete_domain))
        // User routes
        .route("/api/v1/domains/:domain/users", get(handlers::list_users))
        .route("/api/v1/domains/:domain/users", post(handlers::create_user))
        .route(
            "/api/v1/domains/:domain/users/:username",
            get(handlers::get_user),
        )
        .route(
            "/api/v1/domains/:domain/users/:username",
            put(handlers::update_user),
        )
        .route(
            "/api/v1/domains/:domain/users/:username",
            delete(handlers::delete_user),
        )
        .layer(middleware::from_fn(move |req: Request, next: Next| {
            let api_key = api_key.clone();
            async move { auth_middleware(req, next, api_key).await }
        }))
        .with_state(state)
}
