//! API request handlers

use std::sync::Arc;

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use serde::{Deserialize, Serialize};

use crate::auth::{AuthStore, ImapForwardConfig, User};
use crate::vhost::domain::{DomainConfig, DomainInfo, DomainTlsConfig};
use crate::vhost::VirtualHostRouter;

/// Application state shared across handlers
pub struct AppState {
    pub auth_store: Arc<AuthStore>,
    pub vhost_router: Arc<VirtualHostRouter>,
}

/// Health check response
#[derive(Serialize)]
pub struct HealthResponse {
    pub status: String,
    pub version: String,
}

/// Domain response
#[derive(Serialize)]
pub struct DomainResponse {
    pub name: String,
    pub enabled: bool,
    pub default_storage: String,
    pub maildir_base: String,
}

/// User response (without password hash)
#[derive(Serialize)]
pub struct UserResponse {
    pub username: String,
    pub domain: String,
    pub storage: String,
    pub enabled: bool,
    pub webhook_url: String,
}

/// Create/Update domain request
#[derive(Deserialize)]
pub struct CreateDomainRequest {
    pub name: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_storage")]
    pub default_storage: String,
    #[serde(default)]
    pub maildir_base: String,
    #[serde(default)]
    pub cert_path: String,
    #[serde(default)]
    pub key_path: String,
}

fn default_true() -> bool {
    true
}

fn default_storage() -> String {
    "maildir".to_string()
}

/// Create user request
#[derive(Deserialize)]
pub struct CreateUserRequest {
    pub username: String,
    pub password: String,
    #[serde(default = "default_storage")]
    pub storage: String,
    #[serde(default)]
    pub maildir: String,
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
}

/// Update user request
#[derive(Deserialize)]
pub struct UpdateUserRequest {
    #[serde(default)]
    pub password: Option<String>,
    #[serde(default)]
    pub storage: Option<String>,
    #[serde(default)]
    pub maildir: Option<String>,
    #[serde(default)]
    pub webhook_url: Option<String>,
    #[serde(default)]
    pub enabled: Option<bool>,
}

/// Error response
#[derive(Serialize)]
pub struct ErrorResponse {
    pub error: String,
}

impl ErrorResponse {
    pub fn new(message: &str) -> Self {
        Self {
            error: message.to_string(),
        }
    }
}

/// Health check handler
pub async fn health() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok".to_string(),
        version: crate::VERSION.to_string(),
    })
}

/// List all domains
pub async fn list_domains(
    State(state): State<Arc<AppState>>,
) -> Json<Vec<DomainResponse>> {
    let domains = state.vhost_router.list_domains().await;
    let mut response = Vec::new();

    for name in domains {
        if let Some(config) = state.vhost_router.get_domain(&name).await {
            response.push(DomainResponse {
                name: config.domain.name.clone(),
                enabled: config.domain.enabled,
                default_storage: config.domain.default_storage.clone(),
                maildir_base: config.domain.maildir_base.clone(),
            });
        }
    }

    Json(response)
}

/// Get a specific domain
pub async fn get_domain(
    State(state): State<Arc<AppState>>,
    Path(domain): Path<String>,
) -> Result<Json<DomainResponse>, (StatusCode, Json<ErrorResponse>)> {
    match state.vhost_router.get_domain(&domain).await {
        Some(config) => Ok(Json(DomainResponse {
            name: config.domain.name.clone(),
            enabled: config.domain.enabled,
            default_storage: config.domain.default_storage.clone(),
            maildir_base: config.domain.maildir_base.clone(),
        })),
        None => Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse::new("Domain not found")),
        )),
    }
}

/// Create a new domain
pub async fn create_domain(
    State(state): State<Arc<AppState>>,
    Json(req): Json<CreateDomainRequest>,
) -> Result<(StatusCode, Json<DomainResponse>), (StatusCode, Json<ErrorResponse>)> {
    let config = DomainConfig {
        domain: DomainInfo {
            name: req.name.clone(),
            enabled: req.enabled,
            default_storage: req.default_storage.clone(),
            maildir_base: req.maildir_base.clone(),
        },
        tls: DomainTlsConfig {
            cert_path: req.cert_path,
            key_path: req.key_path,
        },
    };

    state.vhost_router.add_domain(config).await;

    Ok((
        StatusCode::CREATED,
        Json(DomainResponse {
            name: req.name,
            enabled: req.enabled,
            default_storage: req.default_storage,
            maildir_base: req.maildir_base,
        }),
    ))
}

/// Delete a domain
pub async fn delete_domain(
    State(state): State<Arc<AppState>>,
    Path(domain): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    match state.vhost_router.remove_domain(&domain).await {
        Some(_) => Ok(StatusCode::NO_CONTENT),
        None => Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse::new("Domain not found")),
        )),
    }
}

/// List users for a domain
pub async fn list_users(
    State(state): State<Arc<AppState>>,
    Path(domain): Path<String>,
) -> Json<Vec<UserResponse>> {
    let users = state.auth_store.list_users_by_domain(&domain).await;
    let response: Vec<UserResponse> = users
        .into_iter()
        .map(|u| UserResponse {
            username: u.username,
            domain: u.domain,
            storage: u.storage,
            enabled: u.enabled,
            webhook_url: u.webhook_url,
        })
        .collect();
    Json(response)
}

/// Get a specific user
pub async fn get_user(
    State(state): State<Arc<AppState>>,
    Path((domain, username)): Path<(String, String)>,
) -> Result<Json<UserResponse>, (StatusCode, Json<ErrorResponse>)> {
    let full_username = format!("{}@{}", username, domain);
    match state.auth_store.get_user(&full_username).await {
        Some(user) => Ok(Json(UserResponse {
            username: user.username,
            domain: user.domain,
            storage: user.storage,
            enabled: user.enabled,
            webhook_url: user.webhook_url,
        })),
        None => Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse::new("User not found")),
        )),
    }
}

/// Create a new user
pub async fn create_user(
    State(state): State<Arc<AppState>>,
    Path(domain): Path<String>,
    Json(req): Json<CreateUserRequest>,
) -> Result<(StatusCode, Json<UserResponse>), (StatusCode, Json<ErrorResponse>)> {
    // Hash the password
    let password_hash = match state.auth_store.hash_password(&req.password) {
        Ok(hash) => hash,
        Err(e) => {
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new(&format!("Failed to hash password: {}", e))),
            ));
        }
    };

    let user = User {
        username: req.username.clone(),
        domain: domain.clone(),
        password_hash,
        maildir: req.maildir,
        storage: req.storage.clone(),
        s3_bucket: req.s3_bucket,
        s3_prefix: req.s3_prefix,
        webhook_url: req.webhook_url.clone(),
        webhook_events: req.webhook_events,
        enabled: req.enabled,
        imap_forward: ImapForwardConfig::default(),
    };

    state.auth_store.upsert_user(user).await;

    Ok((
        StatusCode::CREATED,
        Json(UserResponse {
            username: req.username,
            domain,
            storage: req.storage,
            enabled: req.enabled,
            webhook_url: req.webhook_url,
        }),
    ))
}

/// Update a user
pub async fn update_user(
    State(state): State<Arc<AppState>>,
    Path((domain, username)): Path<(String, String)>,
    Json(req): Json<UpdateUserRequest>,
) -> Result<Json<UserResponse>, (StatusCode, Json<ErrorResponse>)> {
    let full_username = format!("{}@{}", username, domain);
    let mut user = match state.auth_store.get_user(&full_username).await {
        Some(u) => u,
        None => {
            return Err((
                StatusCode::NOT_FOUND,
                Json(ErrorResponse::new("User not found")),
            ));
        }
    };

    // Update fields
    if let Some(password) = req.password {
        match state.auth_store.hash_password(&password) {
            Ok(hash) => user.password_hash = hash,
            Err(e) => {
                return Err((
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ErrorResponse::new(&format!("Failed to hash password: {}", e))),
                ));
            }
        }
    }
    if let Some(storage) = req.storage {
        user.storage = storage;
    }
    if let Some(maildir) = req.maildir {
        user.maildir = maildir;
    }
    if let Some(webhook_url) = req.webhook_url {
        user.webhook_url = webhook_url;
    }
    if let Some(enabled) = req.enabled {
        user.enabled = enabled;
    }

    state.auth_store.upsert_user(user.clone()).await;

    Ok(Json(UserResponse {
        username: user.username,
        domain: user.domain,
        storage: user.storage,
        enabled: user.enabled,
        webhook_url: user.webhook_url,
    }))
}

/// Delete a user
pub async fn delete_user(
    State(state): State<Arc<AppState>>,
    Path((domain, username)): Path<(String, String)>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    match state.auth_store.remove_user(&username, &domain).await {
        Some(_) => Ok(StatusCode::NO_CONTENT),
        None => Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse::new("User not found")),
        )),
    }
}
