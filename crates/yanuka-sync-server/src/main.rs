//! Boot the sync server.
//!
//! Configuration is entirely environment variables, because the deployment
//! target is a managed container platform where that is the only channel that
//! exists. Every one of them is checked at startup and the process refuses to
//! run without them — a sync server that comes up with no enrolment secret
//! would accept any device that asked.

use std::sync::Arc;

use sqlx::postgres::PgPoolOptions;
use yanuka_sync_server::{migrate, router, AppState};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "yanuka_sync_server=info,tower_http=info".into()),
        )
        .init();

    let database_url = require("DATABASE_URL")?;
    let enrolment_secret = require("YANUKA_ENROLMENT_SECRET")?;
    if enrolment_secret.len() < 24 {
        return Err("YANUKA_ENROLMENT_SECRET must be at least 24 characters".into());
    }

    let port: u16 = std::env::var("PORT").unwrap_or_else(|_| "8080".into()).parse()?;

    let pool = PgPoolOptions::new().max_connections(5).connect(&database_url).await?;
    migrate(&pool).await?;

    let state = Arc::new(AppState { pool, enrolment_secret });
    let listener = tokio::net::TcpListener::bind(("0.0.0.0", port)).await?;
    tracing::info!(port, "yanuka sync server listening");

    axum::serve(listener, router(state)).with_graceful_shutdown(shutdown()).await?;
    Ok(())
}

fn require(key: &str) -> Result<String, String> {
    std::env::var(key).map_err(|_| format!("{key} is required"))
}

/// Finish in-flight requests before exiting.
///
/// A push interrupted mid-transaction is safe — it rolls back and the device
/// retries — but a clean shutdown means the common case never has to rely on
/// that.
async fn shutdown() {
    let _ = tokio::signal::ctrl_c().await;
    tracing::info!("shutting down");
}
