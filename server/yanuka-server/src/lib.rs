//! אוצר שלמה — the sync server.
//!
//! A peer that is always on (ADR-039, docs/SYNC.md). It holds a copy of the
//! archive in the same SQLite schema the desktop uses, accepts revisions from
//! paired devices, and streams accepted changes back to the others. It never
//! merges: a push whose base is stale is refused with the current version,
//! and the device — where a person can decide — does the merging.
//!
//! Everything interesting lives in `yanuka_db::sync`; this binary is the
//! HTTP skin, the device authentication and the command line.
//!
//! The binary (`main.rs`) reads the environment and the command line; this
//! library is the router, so a test can serve it on a random port.

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use axum::extract::{FromRequestParts, Path, Query, State};
use axum::http::request::Parts;
use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::{json, Value};
use yanuka_db::rusqlite::Connection;
use yanuka_db::sync::devices::{self, DeviceInfo};
use yanuka_db::sync::{hub, PushItem};
use yanuka_db::{migrate, DbError};

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Where the server keeps its state. All of it comes from the environment
/// so the same binary runs under Docker, systemd or a Windows service.
pub struct Settings {
    pub data_dir: PathBuf,
    pub bind: SocketAddr,
    pub name: String,
    pub key: Option<String>,
}

impl Settings {
    pub fn from_env() -> Result<Self, String> {
        let data_dir = std::env::var("YANUKA_DATA_DIR").unwrap_or_else(|_| "data".into());
        let bind = std::env::var("YANUKA_BIND").unwrap_or_else(|_| "0.0.0.0:8787".into());
        let bind: SocketAddr =
            bind.parse().map_err(|error| format!("YANUKA_BIND {bind:?}: {error}"))?;
        let name = std::env::var("YANUKA_SERVER_NAME").unwrap_or_else(|_| "אוצר שלמה".into());
        let key = std::env::var("YANUKA_DB_KEY").ok().filter(|key| !key.is_empty());
        Ok(Self { data_dir: PathBuf::from(data_dir), bind, name, key })
    }
}

#[derive(Clone)]
pub struct AppState {
    pub db: Arc<Mutex<Connection>>,
    pub name: Arc<String>,
}

impl AppState {
    pub fn new(connection: Connection, name: &str) -> Self {
        Self { db: Arc::new(Mutex::new(connection)), name: Arc::new(name.to_string()) }
    }

    /// Run a piece of database work off the async threads. SQLite calls are
    /// short; the lock is what serializes them.
    async fn with<T: Send + 'static>(
        &self,
        f: impl FnOnce(&mut Connection) -> Result<T, DbError> + Send + 'static,
    ) -> Result<T, ApiError> {
        let db = self.db.clone();
        tokio::task::spawn_blocking(move || {
            let mut guard = db.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
            f(&mut guard)
        })
        .await
        .map_err(|error| ApiError::internal(format!("worker: {error}")))?
        .map_err(ApiError::from)
    }
}

// -- errors ------------------------------------------------------------------

struct ApiError {
    status: StatusCode,
    code: &'static str,
    message: String,
}

impl ApiError {
    fn internal(message: String) -> Self {
        Self { status: StatusCode::INTERNAL_SERVER_ERROR, code: "database", message }
    }
    fn unauthorized() -> Self {
        Self::from(DbError::Unauthorized)
    }
}

impl From<DbError> for ApiError {
    fn from(error: DbError) -> Self {
        let status = match &error {
            DbError::Unauthorized => StatusCode::UNAUTHORIZED,
            DbError::NotFound(_) => StatusCode::NOT_FOUND,
            DbError::Validation(_) | DbError::Serde(_) => StatusCode::BAD_REQUEST,
            _ => StatusCode::INTERNAL_SERVER_ERROR,
        };
        Self { status, code: error.code(), message: error.to_string() }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (self.status, Json(json!({ "code": self.code, "message": self.message }))).into_response()
    }
}

// -- authentication ----------------------------------------------------------

/// The device behind a request, resolved from its bearer token.
struct Device(String);

impl FromRequestParts<AppState> for Device {
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, ApiError> {
        let token = parts
            .headers
            .get(header::AUTHORIZATION)
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.strip_prefix("Bearer "))
            .map(str::trim)
            .filter(|token| !token.is_empty())
            .map(str::to_string)
            .ok_or_else(ApiError::unauthorized)?;
        let id = state.with(move |connection| devices::authenticate(connection, &token)).await?;
        Ok(Device(id))
    }
}

// -- handlers ----------------------------------------------------------------

async fn health(State(state): State<AppState>) -> Json<Value> {
    Json(json!({ "name": *state.name, "version": VERSION, "status": "ok" }))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PairRequest {
    code: String,
    device: DeviceInfo,
}

async fn pair(
    State(state): State<AppState>,
    Json(request): Json<PairRequest>,
) -> Result<Json<Value>, ApiError> {
    let device_id = request.device.id.clone();
    let token = state
        .with(move |connection| {
            devices::redeem_pair_code(connection, &request.code, &request.device)
        })
        .await?;
    eprintln!("paired device {device_id}");
    Ok(Json(json!({ "token": token, "serverName": *state.name, "deviceId": device_id })))
}

async fn pair_code(
    State(state): State<AppState>,
    _device: Device,
) -> Result<Json<Value>, ApiError> {
    let (code, expires_at) = state.with(|connection| devices::create_pair_code(connection)).await?;
    Ok(Json(json!({ "code": code, "expiresAt": expires_at })))
}

#[derive(Deserialize)]
struct PushRequest {
    items: Vec<PushItem>,
}

async fn push(
    State(state): State<AppState>,
    Device(device): Device,
    Json(request): Json<PushRequest>,
) -> Result<Json<Value>, ApiError> {
    let results =
        state.with(move |connection| hub::accept(connection, &device, &request.items)).await?;
    Ok(Json(json!({ "results": results })))
}

#[derive(Deserialize)]
struct PullQuery {
    #[serde(default)]
    after: i64,
    #[serde(default = "default_limit")]
    limit: usize,
}

fn default_limit() -> usize {
    200
}

async fn pull(
    State(state): State<AppState>,
    Device(device): Device,
    Query(query): Query<PullQuery>,
) -> Result<Json<Value>, ApiError> {
    let page = state
        .with(move |connection| hub::pull(connection, query.after, query.limit, &device))
        .await?;
    Ok(Json(serde_json::to_value(page).map_err(DbError::from)?))
}

async fn status(State(state): State<AppState>, _device: Device) -> Result<Json<Value>, ApiError> {
    let mut summary = state.with(|connection| hub::summary(connection)).await?;
    summary["name"] = json!(*state.name);
    summary["version"] = json!(VERSION);
    Ok(Json(summary))
}

async fn list_devices(
    State(state): State<AppState>,
    _device: Device,
) -> Result<Json<Value>, ApiError> {
    let list = state.with(|connection| devices::list(connection)).await?;
    Ok(Json(json!(list)))
}

async fn revoke_device(
    State(state): State<AppState>,
    Device(caller): Device,
    Path(id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    if caller == id {
        return Err(DbError::Validation(
            "מכשיר אינו יכול לנתק את עצמו — עשו זאת ממכשיר אחר".into(),
        )
        .into());
    }
    state.with(move |connection| devices::revoke(connection, &id)).await?;
    Ok(Json(json!({})))
}

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/api/health", get(health))
        .route("/api/pair", post(pair))
        .route("/api/pair-codes", post(pair_code))
        .route("/api/sync/push", post(push))
        .route("/api/sync/pull", get(pull))
        .route("/api/status", get(status))
        .route("/api/devices", get(list_devices))
        .route("/api/devices/{id}/revoke", post(revoke_device))
        // A first pairing can carry a whole archive in a few pushes.
        .layer(axum::extract::DefaultBodyLimit::max(32 * 1024 * 1024))
        .with_state(state)
}

// -- the database ------------------------------------------------------------

/// Open (or create) the server's copy, keyed when `YANUKA_DB_KEY` is set and
/// the binary was built with `sqlcipher`.
pub fn open_database(settings: &Settings) -> Result<Connection, String> {
    std::fs::create_dir_all(&settings.data_dir)
        .map_err(|error| format!("{}: {error}", settings.data_dir.display()))?;
    let path = settings.data_dir.join("otzar-shlomo.db");
    let pragma = match &settings.key {
        Some(hex) => {
            Some(yanuka_db::encryption::raw_key_pragma(hex).map_err(|error| error.to_string())?)
        }
        None => None,
    };
    let mut connection =
        yanuka_db::open(&path, pragma.as_deref()).map_err(|error| error.to_string())?;
    yanuka_db::connection::assert_capabilities(&connection).map_err(|error| error.to_string())?;
    let applied = migrate(&mut connection).map_err(|error| error.to_string())?;
    if applied > 0 {
        eprintln!("applied {applied} migration(s) to {}", path.display());
    }
    // Deliberately no default categories: the server holds what devices
    // tell it, and nothing they would then collide with.
    Ok(connection)
}
