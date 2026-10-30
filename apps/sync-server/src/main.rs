#![forbid(unsafe_code)]

use std::{env, error::Error, net::SocketAddr};

use axum::{extract::State, http::StatusCode, routing::get, Json, Router};
use serde::Serialize;
use sqlx::{postgres::PgPoolOptions, PgPool};
use tokio::net::TcpListener;
use tracing_subscriber::EnvFilter;
use uuid::Uuid;

const API_VERSION: &str = "v1";

#[derive(Clone)]
struct AppState {
    database: PgPool,
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

    let state = AppState { database };
    let app = Router::new()
        .route("/health/live", get(liveness))
        .route("/health/ready", get(readiness))
        .route("/api/v1/server-info", get(server_info))
        .with_state(state);

    let listener = TcpListener::bind(bind_address).await?;
    tracing::info!(address = %bind_address, api_version = API_VERSION, "Apex Sync Server is ready");
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    Ok(())
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
