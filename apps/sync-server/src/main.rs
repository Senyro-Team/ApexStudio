#![forbid(unsafe_code)]

mod auth;
mod sync;

use std::{env, error::Error, net::SocketAddr, path::PathBuf, sync::Arc, time::Instant};

use axum::{
    extract::{DefaultBodyLimit, MatchedPath, Request, State},
    http::StatusCode,
    middleware::{self, Next},
    response::Response,
    routing::get,
    Json, Router,
};
use serde::Serialize;
use sha2::{Digest, Sha256};
use sqlx::{postgres::PgPoolOptions, PgPool};
use tokio::net::TcpListener;
use tokio::sync::Semaphore;
use tracing_subscriber::EnvFilter;
use uuid::Uuid;

const API_VERSION: &str = "v1";

#[derive(Clone)]
pub struct AppState {
    pub database: PgPool,
    pub bootstrap_token_hash: Option<[u8; 32]>,
    pub password_hash_slots: Arc<Semaphore>,
    pub storage_dir: PathBuf,
}

#[derive(Serialize)]
struct HealthResponse {
    status: &'static str,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ServerInfo {
    product: &'static str,
    api_version: &'static str,
    server_version: &'static str,
    server_instance_id: String,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error + Send + Sync>> {
    init_logging();

    let bind_address = env_value("APEX_SYNC_BIND", "0.0.0.0:8080")?.parse::<SocketAddr>()?;
    let database_url = env::var("APEX_SYNC_DATABASE_URL").map_err(|_| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "APEX_SYNC_DATABASE_URL must be set",
        )
    })?;
    let database = PgPoolOptions::new()
        .max_connections(20)
        .connect(&database_url)
        .await?;

    let migrations = sqlx::migrate::Migrator::new(std::path::Path::new("migrations")).await?;
    migrations.run(&database).await?;
    ensure_server_instance(&database).await?;
    let storage_dir = PathBuf::from(env_value("APEX_SYNC_DATA_DIR", "/data")?);
    tokio::fs::create_dir_all(storage_dir.join("blobs")).await?;
    tokio::fs::create_dir_all(storage_dir.join("staging")).await?;
    cleanup_expired_blob_uploads(&database, &storage_dir).await?;

    let bootstrap_token_hash = match env::var("APEX_SYNC_BOOTSTRAP_TOKEN") {
        Ok(token) if token.is_empty() => None,
        Ok(token) if token.len() >= 32 => Some(Sha256::digest(token.as_bytes()).into()),
        Ok(_) => {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "APEX_SYNC_BOOTSTRAP_TOKEN must contain at least 32 characters",
            )
            .into())
        }
        Err(std::env::VarError::NotPresent) => None,
        Err(error) => return Err(error.into()),
    };

    let state = AppState {
        database,
        bootstrap_token_hash,
        password_hash_slots: Arc::new(Semaphore::new(4)),
        storage_dir,
    };
    let app = Router::new()
        .layer(DefaultBodyLimit::max(2 * 1024 * 1024 + 128 * 1024))
        .route("/health/live", get(liveness))
        .route("/health/ready", get(readiness))
        .route("/api/v1/server-info", get(server_info))
        .merge(auth::public_routes())
        .merge(auth::private_routes(state.clone()))
        .merge(auth::protect(sync::private_routes(), state.clone()))
        .layer(middleware::from_fn(log_failed_requests))
        .with_state(state);

    let listener = TcpListener::bind(bind_address).await?;
    tracing::info!(address = %bind_address, api_version = API_VERSION, "Apex Sync Server is ready");
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    Ok(())
}

async fn log_failed_requests(request: Request, next: Next) -> Response {
    let method = request.method().clone();
    let path = request
        .extensions()
        .get::<MatchedPath>()
        .map(|matched| matched.as_str().to_owned())
        .unwrap_or_else(|| request.uri().path().to_owned());
    let started = Instant::now();
    let response = next.run(request).await;
    let status = response.status();
    if status.is_server_error() {
        tracing::error!(
            event = "http_request_failed",
            method = %method,
            path = %path,
            status = status.as_u16(),
            elapsed_ms = started.elapsed().as_millis() as u64,
            "Sync API request failed"
        );
    } else if status.is_client_error() {
        tracing::warn!(
            event = "http_request_failed",
            method = %method,
            path = %path,
            status = status.as_u16(),
            elapsed_ms = started.elapsed().as_millis() as u64,
            "Sync API request was rejected"
        );
    }
    response
}

fn init_logging() {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    tracing_subscriber::fmt().with_env_filter(filter).init();
}

fn env_value(key: &str, default: &str) -> Result<String, std::env::VarError> {
    match env::var(key) {
        Ok(value) if !value.trim().is_empty() => Ok(value),
        Ok(_) => Ok(default.to_owned()),
        Err(std::env::VarError::NotPresent) => Ok(default.to_owned()),
        Err(error) => Err(error),
    }
}

async fn ensure_server_instance(database: &PgPool) -> Result<(), sqlx::Error> {
    let candidate_id = Uuid::now_v7();
    sqlx::query(
        "INSERT INTO server_instance (id, instance_id) VALUES (1, $1) ON CONFLICT (id) DO NOTHING",
    )
    .bind(candidate_id)
    .execute(database)
    .await?;
    Ok(())
}

async fn cleanup_expired_blob_uploads(
    database: &PgPool,
    storage_dir: &std::path::Path,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let expired = sqlx::query_scalar::<_, Uuid>(
        "DELETE FROM blob_uploads WHERE expires_at<=now() RETURNING id",
    )
    .fetch_all(database)
    .await?;
    for upload_id in expired {
        let path = storage_dir
            .join("staging")
            .join("uploads")
            .join(upload_id.to_string());
        let _ = tokio::fs::remove_dir_all(path).await;
    }
    Ok(())
}

async fn liveness() -> Json<HealthResponse> {
    Json(HealthResponse { status: "alive" })
}

async fn readiness(State(state): State<AppState>) -> Result<Json<HealthResponse>, StatusCode> {
    sqlx::query_scalar::<_, i32>("SELECT 1")
        .fetch_one(&state.database)
        .await
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;

    Ok(Json(HealthResponse { status: "ready" }))
}

async fn server_info(State(state): State<AppState>) -> Result<Json<ServerInfo>, StatusCode> {
    let instance_id =
        sqlx::query_scalar::<_, Uuid>("SELECT instance_id FROM server_instance WHERE id = 1")
            .fetch_one(&state.database)
            .await
            .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;

    Ok(Json(ServerInfo {
        product: "Apex Sync Server",
        api_version: API_VERSION,
        server_version: env!("CARGO_PKG_VERSION"),
        server_instance_id: instance_id.to_string(),
    }))
}

async fn shutdown_signal() {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{signal, SignalKind};

        if let Ok(mut terminate) = signal(SignalKind::terminate()) {
            tokio::select! {
                _ = tokio::signal::ctrl_c() => {},
                _ = terminate.recv() => {},
            }
            return;
        }
    }

    let _ = tokio::signal::ctrl_c().await;
}
