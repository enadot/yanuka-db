//! Syncing without being asked.
//!
//! Until now syncing was a button in הגדרות. That was survivable while the only
//! device was a desktop with a person sitting at it, and it stops being
//! survivable the moment a phone is in the picture — nobody opens a settings
//! screen to press a button, so a phone that syncs only on demand holds contacts
//! that are quietly out of date, and the staleness is invisible until the moment
//! it costs something.
//!
//! The loop is deliberately unassertive. It never reports being offline, which
//! is the ordinary state of this application rather than an incident; it backs
//! off while nothing is reachable and snaps back the moment something is (see
//! `schedule`); and it emits an event only when something actually moved, so the
//! interface refreshes on real news and not on a heartbeat.

use tauri::{AppHandle, Emitter, Manager};
use yanuka_sync_client::{schedule, sync_once};

use crate::state::AppState;

/// The event the frontend listens for. Carries the outcome, so a screen can
/// decide whether the change is worth telling the user about.
pub const CHANGED: &str = "sync:changed";

pub fn spawn(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        let mut wait = schedule::AFTER_LAUNCH;
        loop {
            tokio::time::sleep(wait).await;

            let state = app.state::<AppState>();
            let stored = state.with(|connection| yanuka_sync_client::load(connection));
            let mut settings = match stored {
                Ok(Some(settings)) => settings,
                // Not connected — which is the expected state for the machine
                // this was built for. Keep a slow pulse rather than stopping, so
                // connecting in settings starts working without a restart.
                Ok(None) => {
                    wait = schedule::LONGEST_RETRY;
                    continue;
                }
                Err(error) => {
                    eprintln!("sync: could not read settings: {error}");
                    wait = schedule::LONGEST_RETRY;
                    continue;
                }
            };

            // One sync at a time. Two rounds racing would push the same changes
            // twice — harmless, the mutation id makes the second a no-op — but
            // they would also write the cursor over each other, and the loser
            // would re-fetch a page for nothing.
            let outcome = {
                let _gate = state.sync_gate().lock().await;
                sync_once(&*state, &mut settings).await
            };

            wait = schedule::next(&outcome, wait);

            match &outcome {
                Ok(outcome) if outcome.applied > 0 || outcome.pushed > 0 => {
                    let _ = app.emit(CHANGED, outcome);
                }
                // Nothing moved. Emitting anyway would make every screen refetch
                // on a timer forever, which is a lot of work to display the same
                // thing.
                Ok(_) => {}
                Err(yanuka_sync_client::SyncError::Offline) => {}
                Err(error) => eprintln!("sync: {error}"),
            }
        }
    });
}
