//! The sync server: an ordered log of sealed envelopes, and nothing else.
//!
//! It does not know what a contact is. There is no schema here mirroring the
//! desktop's, no merge logic, no search. A device pushes sealed envelopes and
//! pulls the ones it has not seen; every decision that could lose or corrupt
//! data was made in `yanuka-db::apply`, on the device, in code that runs
//! identically whether or not this server exists.
//!
//! That is a deliberate choice and worth defending, because the obvious
//! alternative — a server holding a full replica, applying changes, resolving
//! conflicts — is what most sync systems look like:
//!
//! * A second schema is a second implementation of every rule about this data,
//!   in a different language against a different database. The two drift. When
//!   they drift, the symptom is a contact that looks different depending on
//!   which device you ask, and no single place to go and read what is correct.
//!
//! * Merging on the server means the server can read the archive. Sealing the
//!   payloads is what makes it defensible to keep a private contact list on
//!   rented infrastructure, and it is incompatible with a server that has
//!   opinions about the contents.
//!
//! * The hard part — three-way merge, conflict detection, tombstones, deferral
//!   — is already written and tested against two real databases. Rewriting it
//!   here would double the surface without adding a capability.
//!
//! What is left is small enough to read in one sitting, which is the property
//! that matters for the piece of the system nobody will look at again until it
//! misbehaves.

use std::sync::Arc;

use axum::extract::{Query, State};
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use sqlx::postgres::PgPool;
use subtle::ConstantTimeEq;
use yanuka_sync_proto::{
    Cursor, Envelope, PullResponse, PushRequest, PushResponse, RegisterRequest, RegisterResponse,
};

/// Ceiling on one pull, so a device that has been away for a year receives its
/// backlog in pages rather than in one response that times out halfway.
const MAX_PAGE: i64 = 500;

/// Ceiling on one push. Bounds the work done while the write lock is held.
const MAX_PUSH: usize = 500;

/// Serialises pushes. See `push` for why this is not optional.
///
/// Public so the test suite can hold it and prove that a push actually waits —
/// the property is invisible in the stored rows, so the only way to check it is
/// to take the lock and watch a push block.
pub const WRITE_LOCK: i64 = 0x59_41_4e_55; // "YANU"

#[derive(Debug, thiserror::Error)]
pub enum ServerError {
    #[error("database: {0}")]
    Database(#[from] sqlx::Error),
    #[error("unauthorised")]
    Unauthorised,
    #[error("{0}")]
    BadRequest(String),
}

impl IntoResponse for ServerError {
    fn into_response(self) -> Response {
        let (status, message) = match &self {
            // The reason a token was rejected is not the caller's business:
            // distinguishing "no such device" from "revoked" tells someone
            // probing which tokens once existed.
            ServerError::Unauthorised => (StatusCode::UNAUTHORIZED, "unauthorised".to_string()),
            ServerError::BadRequest(message) => (StatusCode::BAD_REQUEST, message.clone()),
            ServerError::Database(error) => {
                // Logged in full, returned as nothing. A database error message
                // can carry column names, constraint names and fragments of
                // values.
                tracing::error!(%error, "database error");
                (StatusCode::INTERNAL_SERVER_ERROR, "internal error".to_string())
            }
        };
        (status, Json(serde_json::json!({ "error": message }))).into_response()
    }
}

type Result<T> = std::result::Result<T, ServerError>;

pub struct AppState {
    pub pool: PgPool,
    /// The secret a new device presents once to enrol. Compared in constant
    /// time; never stored anywhere but the process environment.
    pub enrolment_secret: String,
}

pub type SharedState = Arc<AppState>;

/// Create the tables. Idempotent, run at every start.
///
/// The schema is four columns of metadata and a blob. That it is this small is
/// the design, not an early stage of it.
pub async fn migrate(pool: &PgPool) -> std::result::Result<(), sqlx::Error> {
    sqlx::raw_sql(
        r#"
        CREATE TABLE IF NOT EXISTS devices (
          id           TEXT PRIMARY KEY,
          name         TEXT NOT NULL,
          kind         TEXT NOT NULL,
          token_hash   TEXT NOT NULL UNIQUE,
          created_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
          last_seen_at TIMESTAMPTZ,
          revoked_at   TIMESTAMPTZ
        );

        -- `seq` is the only ordering in the system. `id` is the device's own
        -- mutation id, unique so that a retried push after a lost response is a
        -- no-op rather than a duplicate.
        CREATE TABLE IF NOT EXISTS envelopes (
          seq         BIGSERIAL PRIMARY KEY,
          id          TEXT NOT NULL UNIQUE,
          device_id   TEXT NOT NULL REFERENCES devices (id),
          created_at  TEXT NOT NULL,
          nonce       TEXT NOT NULL,
          ciphertext  TEXT NOT NULL,
          received_at TIMESTAMPTZ NOT NULL DEFAULT now()
        );
        "#,
    )
    .execute(pool)
    .await?;
    Ok(())
}

pub fn router(state: SharedState) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/v1/devices", post(register))
        .route("/v1/mutations", post(push).get(pull))
        .layer(tower_http::limit::RequestBodyLimitLayer::new(8 * 1024 * 1024))
        .layer(tower_http::trace::TraceLayer::new_for_http())
        .with_state(state)
}

async fn health(State(state): State<SharedState>) -> Result<Json<serde_json::Value>> {
    // Touches the database rather than just returning 200: a container that is
    // up but cannot reach Postgres is down, and a health check that cannot tell
    // the difference will keep it in the load balancer.
    let count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM devices").fetch_one(&state.pool).await?;
    Ok(Json(serde_json::json!({ "status": "ok", "devices": count })))
}

fn hash_token(token: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(token.as_bytes());
    format!("{:x}", hasher.finalize())
}

async fn register(
    State(state): State<SharedState>,
    Json(request): Json<RegisterRequest>,
) -> Result<Json<RegisterResponse>> {
    // Constant time: a byte-by-byte comparison that returns early leaks the
    // secret one character at a time to anyone who can measure the response.
    let presented = request.enrolment_secret.as_bytes();
    let expected = state.enrolment_secret.as_bytes();
    let matches = presented.len() == expected.len() && presented.ct_eq(expected).into();
    if !matches {
        return Err(ServerError::Unauthorised);
    }

    if request.device_name.trim().is_empty() {
        return Err(ServerError::BadRequest("device_name is required".into()));
    }

    let device_id = yanuka_sync_proto::random_secret();
    let token = yanuka_sync_proto::random_secret();

    sqlx::query(
        "INSERT INTO devices (id, name, kind, token_hash, last_seen_at)
         VALUES ($1, $2, $3, $4, now())",
    )
    .bind(&device_id)
    .bind(request.device_name.trim())
    .bind(&request.device_type)
    .bind(hash_token(&token))
    .execute(&state.pool)
    .await?;

    tracing::info!(device = %device_id, name = %request.device_name, "device enrolled");
    Ok(Json(RegisterResponse { device_id, token }))
}

/// Identify the caller, or refuse.
async fn authenticate(state: &SharedState, headers: &HeaderMap) -> Result<String> {
    let token = headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .ok_or(ServerError::Unauthorised)?;

    let device_id: Option<String> =
        sqlx::query_scalar("SELECT id FROM devices WHERE token_hash = $1 AND revoked_at IS NULL")
            .bind(hash_token(token))
            .fetch_optional(&state.pool)
            .await?;

    let device_id = device_id.ok_or(ServerError::Unauthorised)?;
    sqlx::query("UPDATE devices SET last_seen_at = now() WHERE id = $1")
        .bind(&device_id)
        .execute(&state.pool)
        .await?;
    Ok(device_id)
}

async fn push(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Json(request): Json<PushRequest>,
) -> Result<Json<PushResponse>> {
    let device_id = authenticate(&state, &headers).await?;

    if request.envelopes.len() > MAX_PUSH {
        return Err(ServerError::BadRequest(format!("at most {MAX_PUSH} envelopes per push")));
    }

    let mut tx = state.pool.begin().await?;

    // Serialise pushes for the duration of the transaction.
    //
    // Without this, two concurrent pushes can take sequence numbers 10 and 11
    // and commit in the opposite order. A device that pulls in between sees 11,
    // stores 11 as its cursor, and asks next time for everything after 11 —
    // so 10, which committed a moment later, is never delivered to it. Nothing
    // reports an error; a change simply never arrives on one device.
    //
    // At this scale the lock costs nothing. Even at a much larger one, a
    // correct log is worth more than concurrent writers.
    sqlx::query("SELECT pg_advisory_xact_lock($1)").bind(WRITE_LOCK).execute(&mut *tx).await?;

    let mut accepted = Vec::with_capacity(request.envelopes.len());
    for envelope in &request.envelopes {
        if envelope.id.is_empty() || envelope.ciphertext.is_empty() {
            return Err(ServerError::BadRequest("envelope is missing id or ciphertext".into()));
        }

        // ON CONFLICT DO NOTHING, because a push whose response was lost is
        // retried with the same ids. The device is told the id was accepted
        // either way — it *is* stored, which is what the device needs to know
        // before it stops sending it.
        sqlx::query(
            "INSERT INTO envelopes (id, device_id, created_at, nonce, ciphertext)
             VALUES ($1, $2, $3, $4, $5)
             ON CONFLICT (id) DO NOTHING",
        )
        .bind(&envelope.id)
        .bind(&device_id)
        .bind(&envelope.created_at)
        .bind(&envelope.nonce)
        .bind(&envelope.ciphertext)
        .execute(&mut *tx)
        .await?;

        accepted.push(envelope.id.clone());
    }

    let cursor: Cursor = sqlx::query_scalar("SELECT COALESCE(MAX(seq), 0) FROM envelopes")
        .fetch_one(&mut *tx)
        .await?;

    tx.commit().await?;
    Ok(Json(PushResponse { accepted, cursor }))
}

#[derive(Debug, Deserialize)]
pub struct PullParams {
    #[serde(default)]
    pub after: Cursor,
    pub limit: Option<i64>,
}

async fn pull(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Query(params): Query<PullParams>,
) -> Result<Json<PullResponse>> {
    authenticate(&state, &headers).await?;

    let limit = params.limit.unwrap_or(MAX_PAGE).clamp(1, MAX_PAGE);

    // Deliberately *not* filtered by device.
    //
    // Excluding the caller's own envelopes would save a little bandwidth and
    // cost the ability to rebuild a device from the server: a machine that
    // loses its database and re-enrols would receive everyone's history except
    // its own. The device already recognises a change it has applied and
    // ignores it, so the redundancy is free where it matters and valuable where
    // it does not.
    let rows: Vec<(i64, String, String, String, String, String)> = sqlx::query_as(
        "SELECT seq, id, device_id, created_at, nonce, ciphertext
           FROM envelopes
          WHERE seq > $1
          ORDER BY seq
          LIMIT $2",
    )
    .bind(params.after)
    .bind(limit + 1)
    .fetch_all(&state.pool)
    .await?;

    let has_more = rows.len() as i64 > limit;
    let page = if has_more { &rows[..limit as usize] } else { &rows[..] };

    let cursor = page.last().map(|row| row.0).unwrap_or(params.after);
    let envelopes = page
        .iter()
        .map(|(seq, id, device_id, created_at, nonce, ciphertext)| Envelope {
            id: id.clone(),
            device_id: device_id.clone(),
            created_at: created_at.clone(),
            nonce: nonce.clone(),
            ciphertext: ciphertext.clone(),
            seq: Some(*seq),
        })
        .collect();

    Ok(Json(PullResponse { envelopes, cursor, has_more }))
}
