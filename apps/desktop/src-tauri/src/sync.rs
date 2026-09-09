//! The sync worker: a thread that runs a cycle when there is something to
//! send, when the server may have something new, or when asked.
//!
//! It never holds the database while it talks to the server — the engine
//! borrows the connection step by step through `AppState::with` — so a
//! search or a save on the UI thread is never queued behind a slow link. A
//! machine with no pairing costs nothing: the worker wakes, sees no server,
//! and sleeps again.

use std::sync::mpsc::{sync_channel, Receiver, RecvTimeoutError, SyncSender};
use std::time::{Duration, Instant};

use tauri::{AppHandle, Manager};
use yanuka_db::sync::config::{self, SyncConfig};
use yanuka_db::sync::engine::CycleReport;
use yanuka_db::sync::http::HttpTransport;
use yanuka_db::DbError;

use crate::state::AppState;

/// How often the worker looks for unsent changes.
const POLL: Duration = Duration::from_secs(30);
/// How long a quiet machine waits before asking the server for news anyway.
const IDLE_CYCLE: Duration = Duration::from_secs(5 * 60);
/// Let the window come up before the first cycle.
const FIRST_DELAY: Duration = Duration::from_secs(8);

pub fn spawn(handle: AppHandle) -> SyncSender<()> {
    let (kick, wake) = sync_channel::<()>(1);
    std::thread::Builder::new()
        .name("sync".into())
        .spawn(move || worker(handle, wake))
        .expect("the sync worker thread must start");
    kick
}

fn worker(handle: AppHandle, wake: Receiver<()>) {
    std::thread::sleep(FIRST_DELAY);
    // Due immediately: the machine may have been off for days.
    let mut last = Instant::now() - IDLE_CYCLE;
    let mut kicked = true;
    loop {
        if !kicked {
            kicked = match wake.recv_timeout(POLL) {
                Ok(()) => true,
                Err(RecvTimeoutError::Timeout) => false,
                Err(RecvTimeoutError::Disconnected) => return,
            };
        }
        let state = handle.state::<AppState>();
        let config = state.with(|connection| config::load_config(connection)).ok().flatten();
        let Some(config) = config else {
            kicked = false;
            continue;
        };
        let pending =
            state.with(|connection| yanuka_db::mutation::pending_count(connection)).unwrap_or(0);
        if kicked || pending > 0 || last.elapsed() >= IDLE_CYCLE {
            match run_cycle_now(&state, &config) {
                Ok(report) if report.pushed + report.applied + report.conflicts > 0 => {
                    eprintln!(
                        "sync: pushed {}, applied {}, conflicts {}",
                        report.pushed, report.applied, report.conflicts
                    );
                }
                Ok(_) => {}
                Err(error) => eprintln!("sync: {error}"),
            }
            last = Instant::now();
        }
        kicked = false;
    }
}

/// One cycle, now, on the calling thread. The result is also written to the
/// database (`lastSyncAt` / `lastError`), which is what the screens read.
pub fn run_cycle_now(state: &AppState, config: &SyncConfig) -> Result<CycleReport, DbError> {
    let Some(_guard) = state.try_cycle() else {
        return Err(DbError::Sync("סנכרון כבר מתבצע".into()));
    };
    let transport = HttpTransport::new(&config.server_url, &config.token)?;
    state.set_syncing(true);
    let engine = state.semantic_engine();
    let (report, error) = yanuka_db::sync::run_cycle_partial(state, &transport, engine.as_deref());
    state.set_syncing(false);
    // Meaning-based search and categories see the new text only once the
    // vectors are rebuilt — including what a cycle pulled before its link
    // dropped mid-way, which is why the report is read before the error.
    for contact_id in &report.touched_contacts {
        state.semantic_touch(contact_id);
    }
    match &error {
        None => state.set_online(true),
        Some(DbError::Sync(_)) => state.set_online(false),
        // A refusal is not an outage; the server answered.
        Some(_) => state.set_online(true),
    }
    match error {
        None => Ok(report),
        Some(error) => Err(error),
    }
}

/// The device's own name for the server's list: the machine name when the
/// OS offers one.
pub fn default_device_name() -> String {
    std::env::var("COMPUTERNAME")
        .or_else(|_| std::env::var("HOSTNAME"))
        .ok()
        .map(|name| name.trim().to_string())
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| "המחשב הזה".into())
}
