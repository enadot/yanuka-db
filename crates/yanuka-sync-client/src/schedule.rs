//! When to try again.
//!
//! Syncing on a button press was defensible while the only device was a desktop
//! in front of a person. It stops being defensible the moment a phone is
//! involved: nobody opens a settings screen to press a button, so a phone that
//! only syncs on demand is a phone whose contacts are quietly stale, and the
//! staleness is invisible until it matters.
//!
//! What makes the timing non-trivial is that *this application is offline most
//! of the time by design*. A fixed interval means a machine that has been in a
//! drawer for a week wakes up and fails a network call every few minutes for a
//! week, and the failures are indistinguishable from the ones that matter. So
//! the wait grows while nothing is reachable and snaps back the moment something
//! is — the archive is fully usable throughout, which is the whole point of it.

use std::time::Duration;

/// How long to wait after a round that reached the server.
pub const SETTLED: Duration = Duration::from_secs(5 * 60);

/// The first wait after a failure to reach it.
pub const FIRST_RETRY: Duration = Duration::from_secs(60);

/// The longest this will ever wait. Half an hour: long enough that an offline
/// machine costs nothing, short enough that plugging in a network cable and
/// making a cup of tea is enough for the archive to catch up by itself.
pub const LONGEST_RETRY: Duration = Duration::from_secs(30 * 60);

/// How long before the first attempt after launch.
///
/// Not zero. Startup already opens the database, runs migrations and takes a
/// backup, and a network call competing with that makes the first screen slower
/// for no benefit — nothing has changed in the seconds since the user
/// double-clicked.
pub const AFTER_LAUNCH: Duration = Duration::from_secs(20);

/// The wait after a round, given what the round did and what was waited last.
///
/// `previous` is only consulted on failure; a successful round always returns
/// to the steady interval rather than easing back to it. Reaching the server
/// once is proof the network is there, and a device that has just been given a
/// connection should not spend the next twenty minutes acting as though it
/// might not have one.
pub fn next(outcome: &crate::Result<crate::SyncOutcome>, previous: Duration) -> Duration {
    match outcome {
        Ok(_) => SETTLED,
        // Unreachable is the ordinary state here, not an incident: back off.
        Err(crate::SyncError::Offline) => backoff(previous),
        // The device was refused, or is not configured. Retrying sooner cannot
        // help — both need a person — but the loop keeps running at its slowest
        // pace so that re-connecting in settings starts working without a
        // restart.
        Err(crate::SyncError::Rejected | crate::SyncError::NotConfigured) => LONGEST_RETRY,
        // Anything else is a real fault worth another look before long, but not
        // worth hammering: a server returning 500 to one device will return it
        // to the next attempt too.
        Err(_) => backoff(previous),
    }
}

fn backoff(previous: Duration) -> Duration {
    if previous < FIRST_RETRY {
        return FIRST_RETRY;
    }
    (previous * 2).min(LONGEST_RETRY)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{SyncError, SyncOutcome};

    fn offline() -> crate::Result<SyncOutcome> {
        Err(SyncError::Offline)
    }

    fn reached() -> crate::Result<SyncOutcome> {
        Ok(SyncOutcome::default())
    }

    #[test]
    fn a_machine_that_cannot_reach_the_server_asks_less_and_less_often() {
        let mut wait = AFTER_LAUNCH;
        let mut waits = Vec::new();
        for _ in 0..10 {
            wait = next(&offline(), wait);
            waits.push(wait);
        }

        assert_eq!(waits[0], FIRST_RETRY, "the first retry should be prompt");
        assert!(waits.windows(2).all(|pair| pair[1] >= pair[0]), "the wait went down: {waits:?}");
        assert_eq!(*waits.last().unwrap(), LONGEST_RETRY, "the backoff never settled");
    }

    #[test]
    fn the_wait_is_capped_rather_than_doubling_forever() {
        // Without a cap, a laptop left in a drawer for a month comes out with a
        // next attempt scheduled after the heat death of the user's patience.
        let mut wait = LONGEST_RETRY;
        for _ in 0..50 {
            wait = next(&offline(), wait);
        }
        assert_eq!(wait, LONGEST_RETRY);
    }

    #[test]
    fn one_successful_round_restores_the_normal_pace_immediately() {
        // The moment that matters: the user has just plugged in the cable. A
        // gradual climb back down would leave the archive stale for another
        // twenty minutes for no reason.
        let wait = next(&reached(), LONGEST_RETRY);
        assert_eq!(wait, SETTLED);
    }

    #[test]
    fn a_device_that_was_refused_keeps_a_slow_pulse_rather_than_stopping() {
        // Stopping would mean that re-connecting in settings does nothing until
        // the application is restarted — and nobody would connect the two.
        let wait = next(&Err(SyncError::Rejected), FIRST_RETRY);
        assert_eq!(wait, LONGEST_RETRY);
        assert!(wait < Duration::MAX, "the loop must not stop");
    }

    #[test]
    fn an_unconfigured_device_costs_nothing() {
        assert_eq!(next(&Err(SyncError::NotConfigured), SETTLED), LONGEST_RETRY);
    }
}
