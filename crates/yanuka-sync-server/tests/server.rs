//! The server, end to end, against a real PostgreSQL.
//!
//! Not a mocked store. The things most likely to be wrong in a log server are
//! exactly the things a mock cannot have — sequence assignment under concurrent
//! writers, uniqueness on retry, paging boundaries — so these run the real
//! queries against a real database.
//!
//! Requires `DATABASE_URL` (or `TEST_DATABASE_URL`) to point at a Postgres the
//! test may create and drop tables in. Skipped, loudly, when it is absent: a
//! test that silently passes because it could not run is worse than no test.

use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;
use tower::ServiceExt as _;
use yanuka_sync_proto::{
    Envelope, PullResponse, PushRequest, PushResponse, RegisterRequest, RegisterResponse, SyncKey,
};
use yanuka_sync_server::{migrate, router, AppState, SharedState};

const SECRET: &str = "a-test-enrolment-secret-long-enough";

fn database_url() -> Option<String> {
    std::env::var("TEST_DATABASE_URL").or_else(|_| std::env::var("DATABASE_URL")).ok()
}

/// A fresh, isolated schema per test.
///
/// Every test gets its own Postgres schema rather than sharing tables, because
/// several of them assert on absolute sequence numbers and a leftover row from
/// a neighbour would make them fail in ways that look like real bugs.
async fn fixture(name: &str) -> Option<(SharedState, PgPool)> {
    let url = database_url()?;
    let pool = PgPoolOptions::new().max_connections(5).connect(&url).await.expect("connect");

    // `AssertSqlSafe` because the schema name is a test-supplied constant, not
    // anything reachable from a request.
    sqlx::raw_sql(sqlx::AssertSqlSafe(format!(
        "DROP SCHEMA IF EXISTS {name} CASCADE; CREATE SCHEMA {name}; SET search_path TO {name};"
    )))
    .execute(&pool)
    .await
    .expect("schema");

    // The pool hands out several connections; `search_path` has to be set on
    // each of them, not just the one that created the schema.
    let scoped = PgPoolOptions::new()
        .max_connections(5)
        .after_connect({
            let name = name.to_string();
            move |connection, _| {
                let name = name.clone();
                Box::pin(async move {
                    sqlx::raw_sql(sqlx::AssertSqlSafe(format!("SET search_path TO {name}")))
                        .execute(&mut *connection)
                        .await?;
                    Ok(())
                })
            }
        })
        .connect(&url)
        .await
        .expect("connect scoped");

    migrate(&scoped).await.expect("migrate");
    let state = Arc::new(AppState { pool: scoped.clone(), enrolment_secret: SECRET.to_string() });
    Some((state, pool))
}

macro_rules! require_database {
    ($name:expr) => {
        match fixture($name).await {
            Some(fixture) => fixture,
            None => {
                eprintln!("SKIPPED {}: set DATABASE_URL to a PostgreSQL the tests may use", $name);
                return;
            }
        }
    };
}

async fn call(state: &SharedState, request: Request<Body>) -> (StatusCode, serde_json::Value) {
    let response = router(state.clone()).oneshot(request).await.expect("response");
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), 8 * 1024 * 1024).await.expect("body");
    let json = serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null);
    (status, json)
}

fn json_request(
    method: &str,
    uri: &str,
    token: Option<&str>,
    body: serde_json::Value,
) -> Request<Body> {
    let mut builder =
        Request::builder().method(method).uri(uri).header("content-type", "application/json");
    if let Some(token) = token {
        builder = builder.header("authorization", format!("Bearer {token}"));
    }
    builder.body(Body::from(body.to_string())).expect("request")
}

async fn enrol(state: &SharedState, name: &str) -> RegisterResponse {
    let (status, body) = call(
        state,
        json_request(
            "POST",
            "/v1/devices",
            None,
            serde_json::to_value(RegisterRequest {
                enrolment_secret: SECRET.into(),
                device_name: name.into(),
                device_type: "desktop".into(),
            })
            .unwrap(),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "enrolment failed: {body}");
    serde_json::from_value(body).expect("register response")
}

async fn push(
    state: &SharedState,
    token: &str,
    envelopes: Vec<Envelope>,
) -> (StatusCode, serde_json::Value) {
    call(
        state,
        json_request(
            "POST",
            "/v1/mutations",
            Some(token),
            serde_json::to_value(PushRequest { envelopes }).unwrap(),
        ),
    )
    .await
}

async fn pull(state: &SharedState, token: &str, after: i64) -> PullResponse {
    let (status, body) = call(
        state,
        Request::builder()
            .method("GET")
            .uri(format!("/v1/mutations?after={after}"))
            .header("authorization", format!("Bearer {token}"))
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "pull failed: {body}");
    serde_json::from_value(body).expect("pull response")
}

fn sealed(key: &SyncKey, id: &str, device: &str, plaintext: &str) -> Envelope {
    key.seal(id, device, "2026-08-24T10:00:00Z", plaintext.as_bytes()).unwrap()
}

#[tokio::test]
async fn a_change_pushed_by_one_device_is_pulled_by_another() {
    let (state, _pool) = require_database!("t_round_trip");
    let key = SyncKey::generate();

    let first = enrol(&state, "מחשב ראשי").await;
    let second = enrol(&state, "מחשב שני").await;

    let (status, _) =
        push(&state, &first.token, vec![sealed(&key, "01AAA", &first.device_id, "ירושלים")]).await;
    assert_eq!(status, StatusCode::OK);

    let page = pull(&state, &second.token, 0).await;
    assert_eq!(page.envelopes.len(), 1);
    assert_eq!(page.envelopes[0].id, "01AAA");
    assert!(!page.has_more);

    // And it is still readable at the far end — which is the only thing the
    // whole exercise is for.
    let opened = key.open(&page.envelopes[0]).unwrap();
    assert_eq!(String::from_utf8(opened).unwrap(), "ירושלים");
}

#[tokio::test]
async fn the_server_never_holds_anything_readable() {
    // The claim that justifies putting a private archive on rented hardware.
    // Asserted against the actual stored row, not against the API.
    let (state, _pool) = require_database!("t_opaque");
    let key = SyncKey::generate();
    let device = enrol(&state, "מחשב").await;

    push(&state, &device.token, vec![sealed(&key, "01AAA", &device.device_id, "052-1234567")])
        .await;

    let stored: Vec<(String, String)> = sqlx::query_as("SELECT id, ciphertext FROM envelopes")
        .fetch_all(&state.pool)
        .await
        .unwrap();
    assert_eq!(stored.len(), 1);
    assert!(!stored[0].1.contains("052"), "the phone number is legible in the database");
}

#[tokio::test]
async fn a_wrong_enrolment_secret_is_refused() {
    let (state, _pool) = require_database!("t_enrolment");
    let (status, _) = call(
        &state,
        json_request(
            "POST",
            "/v1/devices",
            None,
            serde_json::json!({
                "enrolmentSecret": "wrong",
                "deviceName": "מתחזה",
                "deviceType": "desktop"
            }),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn pushing_and_pulling_require_a_valid_token() {
    let (state, _pool) = require_database!("t_auth");

    let (status, _) = push(&state, "not-a-real-token", vec![]).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    let (status, _) = call(
        &state,
        Request::builder().method("GET").uri("/v1/mutations?after=0").body(Body::empty()).unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED, "an unauthenticated pull returned data");
}

#[tokio::test]
async fn the_token_is_not_recoverable_from_the_database() {
    // A dump of the devices table must not let someone speak as a device.
    let (state, _pool) = require_database!("t_token_hash");
    let device = enrol(&state, "מחשב").await;

    let hashes: Vec<String> =
        sqlx::query_scalar("SELECT token_hash FROM devices").fetch_all(&state.pool).await.unwrap();
    assert_eq!(hashes.len(), 1);
    assert_ne!(hashes[0], device.token, "the token is stored verbatim");
}

#[tokio::test]
async fn a_repeated_push_does_not_duplicate_the_change() {
    // The normal case, not an exotic one: a push whose response was lost is
    // retried with the same ids. Storing it twice would replay the edit.
    let (state, _pool) = require_database!("t_idempotent");
    let key = SyncKey::generate();
    let device = enrol(&state, "מחשב").await;
    let envelope = sealed(&key, "01AAA", &device.device_id, "once");

    let (_, first) = push(&state, &device.token, vec![envelope.clone()]).await;
    let (_, second) = push(&state, &device.token, vec![envelope]).await;

    let first: PushResponse = serde_json::from_value(first).unwrap();
    let second: PushResponse = serde_json::from_value(second).unwrap();
    // Acknowledged both times — the device needs to know it is stored, and it
    // is — but the log did not grow.
    assert_eq!(first.accepted, vec!["01AAA".to_string()]);
    assert_eq!(second.accepted, vec!["01AAA".to_string()]);

    let count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM envelopes").fetch_one(&state.pool).await.unwrap();
    assert_eq!(count, 1, "a retried push was stored twice");
}

#[tokio::test]
async fn a_device_can_rebuild_itself_from_the_server() {
    // A machine that loses its database re-enrols under a new id and must
    // receive everything, including what it sent before. This is why the pull
    // is not filtered by device — and it makes the server an off-site backup as
    // well as a courier.
    let (state, _pool) = require_database!("t_rebuild");
    let key = SyncKey::generate();

    let original = enrol(&state, "מחשב ראשי").await;
    push(&state, &original.token, vec![sealed(&key, "01AAA", &original.device_id, "לפני האובדן")])
        .await;

    let replacement = enrol(&state, "מחשב ראשי (מותקן מחדש)").await;
    let page = pull(&state, &replacement.token, 0).await;

    assert_eq!(page.envelopes.len(), 1, "the rebuilt device did not get its own history back");
    assert_eq!(key.open(&page.envelopes[0]).unwrap(), "לפני האובדן".as_bytes());
}

#[tokio::test]
async fn a_long_backlog_arrives_in_pages_and_loses_nothing() {
    // The device that has been off the network for months. Paging is where an
    // off-by-one silently drops one change out of a thousand, so the test walks
    // the whole cursor sequence and counts.
    let (state, _pool) = require_database!("t_paging");
    let key = SyncKey::generate();
    let device = enrol(&state, "מחשב").await;

    let total = 1_050;
    for chunk in 0..(total / 350) {
        let envelopes = (0..350)
            .map(|index| {
                let id = format!("01{:05}", chunk * 350 + index);
                sealed(&key, &id, &device.device_id, "x")
            })
            .collect();
        let (status, body) = push(&state, &device.token, envelopes).await;
        assert_eq!(status, StatusCode::OK, "{body}");
    }

    let reader = enrol(&state, "קורא").await;
    let mut cursor = 0;
    let mut seen = Vec::new();
    loop {
        let page = pull(&state, &reader.token, cursor).await;
        seen.extend(page.envelopes.iter().map(|envelope| envelope.id.clone()));
        cursor = page.cursor;
        if !page.has_more {
            break;
        }
    }

    assert_eq!(seen.len(), total as usize, "the backlog lost or repeated something");
    let mut unique = seen.clone();
    unique.sort();
    unique.dedup();
    assert_eq!(unique.len(), total as usize, "a change was delivered twice");
}

#[tokio::test]
async fn a_push_waits_for_the_one_before_it() {
    // The failure being prevented is invisible and permanent, and it is *not*
    // visible in the finished rows — which is why the contiguity test below
    // does not prove it and this one exists.
    //
    // Two pushes take sequence numbers 10 and 11 and commit in the opposite
    // order. A device pulling in between sees 11, stores 11 as its cursor, and
    // never asks for anything below it again. Row 10 commits a moment later and
    // is never delivered to that device. Nothing errors; one change simply
    // never arrives.
    //
    // The advisory lock makes the interleaving impossible, so the test holds
    // the lock and asserts that a push genuinely blocks on it. Delete the
    // `pg_advisory_xact_lock` line in `push` and this fails.
    let (state, _pool) = require_database!("t_serialised");
    let key = SyncKey::generate();
    let device = enrol(&state, "מחשב").await;

    let mut holder = state.pool.acquire().await.unwrap();
    sqlx::raw_sql("BEGIN").execute(&mut *holder).await.unwrap();
    sqlx::query("SELECT pg_advisory_xact_lock($1)")
        .bind(yanuka_sync_server::WRITE_LOCK)
        .execute(&mut *holder)
        .await
        .unwrap();

    let mut attempt = {
        let state = state.clone();
        let token = device.token.clone();
        let envelope = sealed(&key, "01AAA", &device.device_id, "x");
        tokio::spawn(async move { push(&state, &token, vec![envelope]).await })
    };

    let raced = tokio::time::timeout(std::time::Duration::from_millis(400), &mut attempt).await;
    assert!(raced.is_err(), "a push proceeded while the write lock was held");

    sqlx::raw_sql("COMMIT").execute(&mut *holder).await.unwrap();
    drop(holder);

    let (status, body) = tokio::time::timeout(std::time::Duration::from_secs(10), attempt)
        .await
        .expect("the push never completed after the lock was released")
        .expect("push task");
    assert_eq!(status, StatusCode::OK, "{body}");
}

#[tokio::test]
async fn concurrent_pushes_lose_nothing_and_duplicate_nothing() {
    // Four devices pushing at once. This does not prove the ordering property
    // — see the test above for that — but it does prove the log survives real
    // contention without dropping or repeating a change.
    let (state, _pool) = require_database!("t_concurrent");
    let key = SyncKey::generate();

    let mut tokens = Vec::new();
    for index in 0..4 {
        tokens.push(enrol(&state, &format!("מכשיר {index}")).await);
    }

    let mut tasks = Vec::new();
    for (index, device) in tokens.iter().enumerate() {
        let state = state.clone();
        let token = device.token.clone();
        let device_id = device.device_id.clone();
        let key = key.clone();
        tasks.push(tokio::spawn(async move {
            for step in 0..25 {
                let id = format!("01{index}{step:04}");
                let envelope = sealed(&key, &id, &device_id, "x");
                let (status, body) = push(&state, &token, vec![envelope]).await;
                assert_eq!(status, StatusCode::OK, "{body}");
            }
        }));
    }
    for task in tasks {
        task.await.expect("push task");
    }

    // Every sequence number from 1 to the maximum is present and each appears
    // once.
    let sequences: Vec<i64> = sqlx::query_scalar("SELECT seq FROM envelopes ORDER BY seq")
        .fetch_all(&state.pool)
        .await
        .unwrap();
    assert_eq!(sequences.len(), 100);
    for (position, seq) in sequences.iter().enumerate() {
        assert_eq!(*seq, position as i64 + 1, "the sequence has a hole at position {position}");
    }

    // And a reader walking the cursor sees all one hundred.
    let reader = enrol(&state, "קורא").await;
    let page = pull(&state, &reader.token, 0).await;
    assert_eq!(page.envelopes.len(), 100);
}

#[tokio::test]
async fn health_reports_the_database_it_depends_on() {
    let (state, _pool) = require_database!("t_health");
    let (status, body) =
        call(&state, Request::builder().uri("/health").body(Body::empty()).unwrap()).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["status"], "ok");
}
