//! The loop that carries changes between a device and the server.
//!
//! Everything difficult happened elsewhere. `mutation.rs` recorded what changed
//! here, `apply.rs` decided how to fold in what changed there, and the server
//! keeps them in order. This crate is the courier's courier: seal, send, fetch,
//! open, hand to `apply`.
//!
//! It is written so that being interrupted is normal rather than exceptional,
//! because on the machine this is built for it *is* normal — a laptop that is
//! offline for days, a phone on a train, a router that drops a connection
//! mid-response. Three properties hold that together:
//!
//! * **A local change is marked settled only after the server says it stored
//!   it.** Anything else is still pending and will be sent again. Sending twice
//!   is free; the mutation id makes the second delivery a no-op at both ends.
//!
//! * **The cursor advances only past changes that were actually applied.** When
//!   one cannot be applied yet, the cursor stops below it. Advancing past it
//!   would mean never asking for it again — a change that silently never
//!   arrives, which is the exact failure this whole design exists to prevent.
//!
//! * **Nothing here decides anything about the data.** Conflicts are counted
//!   and reported; they are resolved by `apply` and, ultimately, by a person.

use std::time::Duration;

use yanuka_db::apply::{self, Applied, RemoteMutation};
use yanuka_db::rusqlite::Connection;
use yanuka_sync_proto::{
    ConnectionCode, Cursor, Envelope, ProtoError, PullResponse, PushRequest, PushResponse,
    RegisterRequest, RegisterResponse, SyncKey,
};

mod settings;
pub use settings::{clear, load, require, save, SyncSettings};

/// Borrowed access to the local database.
///
/// The loop below takes the database only inside `with`, never across an await.
/// That is not tidiness: the desktop holds the connection behind a mutex that
/// every screen also needs, so a lock held for the duration of an HTTP request
/// would freeze the interface for as long as the network is slow — on the one
/// product whose defining claim is that it works regardless of the network.
pub trait Database {
    fn with<T>(&self, f: impl FnOnce(&mut Connection) -> T) -> T;
}

/// The obvious implementation, for callers that own their connection outright.
pub struct OwnedDatabase(std::sync::Mutex<Connection>);

impl OwnedDatabase {
    pub fn new(connection: Connection) -> Self {
        Self(std::sync::Mutex::new(connection))
    }

    pub fn into_inner(self) -> Connection {
        self.0.into_inner().unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

impl Database for OwnedDatabase {
    fn with<T>(&self, f: impl FnOnce(&mut Connection) -> T) -> T {
        let mut guard = self.0.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        f(&mut guard)
    }
}

/// How many local changes travel in one request.
const PUSH_BATCH: i64 = 200;

/// How many rounds of pull the loop will run before stopping.
///
/// A stop, not a failure: the next sync resumes from the stored cursor. The
/// bound exists so a device with a year of backlog cannot occupy the UI
/// indefinitely on its first run.
const MAX_PULL_ROUNDS: usize = 50;

#[derive(Debug, thiserror::Error)]
pub enum SyncError {
    #[error("סנכרון: אין חיבור לשרת")]
    Offline,
    #[error("סנכרון: המכשיר אינו מחובר. יש להזין קוד חיבור בהגדרות")]
    NotConfigured,
    #[error("סנכרון: השרת דחה את המכשיר. ייתכן שהגישה שלו בוטלה")]
    Rejected,
    #[error("סנכרון: השרת החזיר שגיאה ({0})")]
    Server(u16),
    #[error(transparent)]
    Proto(#[from] ProtoError),
    #[error(transparent)]
    Database(#[from] yanuka_db::error::DbError),
    #[error("סנכרון: {0}")]
    Encoding(String),
}

impl From<serde_json::Error> for SyncError {
    fn from(error: serde_json::Error) -> Self {
        SyncError::Encoding(error.to_string())
    }
}

impl From<reqwest::Error> for SyncError {
    fn from(error: reqwest::Error) -> Self {
        // A timeout or a refused connection is the ordinary state of this
        // application, not an error worth alarming anyone about.
        if error.is_timeout() || error.is_connect() {
            SyncError::Offline
        } else {
            SyncError::Encoding(error.to_string())
        }
    }
}

pub type Result<T> = std::result::Result<T, SyncError>;

/// What one round of syncing did, in terms the settings screen can show.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncOutcome {
    pub pushed: usize,
    pub pulled: usize,
    pub applied: usize,
    /// Changes that landed but disagreed with a local edit. Both values are
    /// kept; a person picks.
    pub conflicts: usize,
    /// Changes held back because what they belong to has not arrived. Normal
    /// mid-backlog, and the reason the cursor did not advance past them.
    pub deferred: usize,
    pub cursor: Cursor,
}

fn client() -> reqwest::Client {
    reqwest::Client::builder()
        // Long enough for a slow connection to finish a page, short enough that
        // a dead network is reported rather than hung on.
        .timeout(Duration::from_secs(30))
        .build()
        .unwrap_or_default()
}

/// Join this device to a server using a pasted connection code.
///
/// The code carries the server address, the enrolment secret and the data key.
/// Only the first two reach the server; the key is stored locally and is what
/// makes the payloads unreadable to it.
pub async fn connect<D: Database>(
    db: &D,
    code: &str,
    device_name: &str,
    device_type: &str,
) -> Result<SyncSettings> {
    let code = ConnectionCode::decode(code)?;
    let http = client();

    let response = http
        .post(format!("{}/v1/devices", code.server_url.trim_end_matches('/')))
        .json(&RegisterRequest {
            enrolment_secret: code.enrolment_secret.clone(),
            device_name: device_name.to_string(),
            device_type: device_type.to_string(),
        })
        .send()
        .await?;

    if response.status() == reqwest::StatusCode::UNAUTHORIZED {
        return Err(SyncError::Rejected);
    }
    if !response.status().is_success() {
        return Err(SyncError::Server(response.status().as_u16()));
    }

    let registered: RegisterResponse = response.json().await?;
    let settings = SyncSettings {
        server_url: code.server_url.trim_end_matches('/').to_string(),
        device_id: registered.device_id,
        token: registered.token,
        key: code.key,
        // Deliberately zero: a device joining an existing archive must receive
        // everything, not only what happens from now on.
        cursor: 0,
        last_sync_at: None,
    };
    db.with(|connection| save(connection, &settings))?;
    Ok(settings)
}

/// Produce a code for adding another device to this same archive.
///
/// The enrolment secret cannot be recovered from what is stored here — the
/// device holds a token, not the secret — so the caller supplies it. That is a
/// real inconvenience and the correct one: a device that could mint enrolment
/// codes from its own credentials would turn any single compromised machine
/// into permission to add more.
pub fn share_code(settings: &SyncSettings, enrolment_secret: &str) -> String {
    ConnectionCode {
        server_url: settings.server_url.clone(),
        enrolment_secret: enrolment_secret.to_string(),
        key: settings.key,
    }
    .encode()
}

/// One full round: send what is local, fetch what is not.
pub async fn sync_once<D: Database>(db: &D, settings: &mut SyncSettings) -> Result<SyncOutcome> {
    let http = client();
    let key = SyncKey::from_bytes(settings.key);
    let mut outcome = SyncOutcome { cursor: settings.cursor, ..Default::default() };

    outcome.pushed = push(db, settings, &http, &key).await?;
    pull(db, settings, &http, &key, &mut outcome).await?;

    settings.last_sync_at = Some(yanuka_db::now_iso());
    db.with(|connection| save(connection, settings))?;
    outcome.cursor = settings.cursor;
    Ok(outcome)
}

async fn push<D: Database>(
    db: &D,
    settings: &SyncSettings,
    http: &reqwest::Client,
    key: &SyncKey,
) -> Result<usize> {
    let mut sent = 0;
    loop {
        let queue = db.with(|connection| apply::pending(connection, PUSH_BATCH))?;
        if queue.is_empty() {
            return Ok(sent);
        }

        let mut envelopes = Vec::with_capacity(queue.len());
        for mutation in &queue {
            let plaintext = serde_json::to_vec(mutation)?;
            envelopes.push(key.seal(
                &mutation.id,
                &settings.device_id,
                &mutation.created_at,
                &plaintext,
            )?);
        }

        let response = http
            .post(format!("{}/v1/mutations", settings.server_url))
            .bearer_auth(&settings.token)
            .json(&PushRequest { envelopes })
            .send()
            .await?;
        check(&response)?;
        let accepted: PushResponse = response.json().await?;

        // Only now. A change marked settled before the server confirmed it
        // would be dropped from the queue and never sent again — the one
        // outcome worse than sending it twice.
        db.with(|connection| apply::mark_synced(connection, &accepted.accepted))?;
        sent += accepted.accepted.len();

        if (queue.len() as i64) < PUSH_BATCH {
            return Ok(sent);
        }
    }
}

async fn pull<D: Database>(
    db: &D,
    settings: &mut SyncSettings,
    http: &reqwest::Client,
    key: &SyncKey,
    outcome: &mut SyncOutcome,
) -> Result<()> {
    for _ in 0..MAX_PULL_ROUNDS {
        let response = http
            .get(format!("{}/v1/mutations", settings.server_url))
            .query(&[("after", settings.cursor.to_string())])
            .bearer_auth(&settings.token)
            .send()
            .await?;
        check(&response)?;
        let page: PullResponse = response.json().await?;

        if page.envelopes.is_empty() {
            return Ok(());
        }
        outcome.pulled += page.envelopes.len();

        // How far the cursor may move. It follows the *applied* changes, not
        // the fetched ones: the first change that could not be applied stops it,
        // so the next pull sees that change again rather than stepping over it.
        let mut safe = settings.cursor;
        let mut blocked = false;

        // One lock for the whole page rather than one per change: applying is
        // local and fast, and a page is a natural unit of work.
        let applied = db.with(|connection| -> Result<()> {
            for envelope in &page.envelopes {
                let seq = envelope.seq.unwrap_or(safe);
                let mutation = decode(key, envelope)?;

                match apply::apply(connection, &mutation)? {
                    Applied::Deferred => {
                        outcome.deferred += 1;
                        blocked = true;
                    }
                    result => {
                        if matches!(result, Applied::Conflicted(_)) {
                            outcome.conflicts += 1;
                        }
                        if !matches!(result, Applied::AlreadySeen) {
                            outcome.applied += 1;
                        }
                        if !blocked {
                            safe = seq;
                        }
                    }
                }
            }
            Ok(())
        });
        applied?;

        // No progress and something still blocked: the missing entity is not
        // going to arrive by asking again. Stop rather than spin — the changes
        // stay in the log, and a later sync that brings the missing contact
        // will let them through.
        if safe == settings.cursor {
            return Ok(());
        }
        settings.cursor = safe;
        db.with(|connection| save(connection, settings))?;

        if !page.has_more {
            return Ok(());
        }
    }
    Ok(())
}

fn decode(key: &SyncKey, envelope: &Envelope) -> Result<RemoteMutation> {
    let plaintext = key.open(envelope)?;
    Ok(serde_json::from_slice(&plaintext)?)
}

fn check(response: &reqwest::Response) -> Result<()> {
    match response.status() {
        status if status.is_success() => Ok(()),
        reqwest::StatusCode::UNAUTHORIZED => Err(SyncError::Rejected),
        status => Err(SyncError::Server(status.as_u16())),
    }
}
