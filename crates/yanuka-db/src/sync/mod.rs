//! Synchronization between devices through a server that is a peer, not an
//! authority (ADR-039; docs/SYNC.md).
//!
//! The unit of exchange is a **revision**: the whole record as it stands
//! (`state`) together with the **fields the sending device changed**
//! (`changed`) and the server version that change was computed against
//! (`base_version`). Whole state is what a device that has never seen the
//! record needs; the changed set is what lets a device that *has* seen it
//! merge field by field. Only a field both sides touched — with different
//! values — becomes a conflict, and a conflict is never resolved by the
//! engine: both values are kept until a person chooses.
//!
//! The module is deliberately free of network code. `Transport` is the whole
//! of what the engine asks of the outside world, and the server side of the
//! protocol (`hub`) is a set of functions over an ordinary connection, so the
//! complete round trip — two devices and a server — runs in one test process
//! against three in-memory databases.

pub mod config;
pub mod conflicts;
pub mod devices;
pub mod engine;
#[cfg(feature = "http")]
pub mod http;
pub mod hub;
pub mod state;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::error::Result;

pub use engine::{run_cycle, run_cycle_partial, CycleReport, Database};

/// Entity types that sync, in the order they must be applied: a contact
/// refers to tags, categories and organizations; notes and relationships
/// refer to contacts. Pushing in this order keeps every reference in the
/// server's stream satisfiable by the time it is read.
pub const ENTITY_ORDER: &[&str] =
    &["tag", "category", "organization", "contact", "note", "relationship"];

/// Position of a type in `ENTITY_ORDER`; unknown types sort last.
pub fn rank(entity_type: &str) -> usize {
    ENTITY_ORDER.iter().position(|t| *t == entity_type).unwrap_or(ENTITY_ORDER.len())
}

/// One record as a device pushes it.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PushItem {
    pub entity_type: String,
    pub entity_id: String,
    /// The server version this change was computed against; 0 for a record
    /// the server has never seen.
    pub base_version: i64,
    /// Field names the device changed since `base_version`.
    pub changed: Vec<String>,
    /// The whole record after the change.
    pub state: Value,
    /// When the change was made on the device (its clock, informational).
    pub created_at: String,
}

/// The server's answer to one pushed item.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PushResult {
    pub entity_type: String,
    pub entity_id: String,
    pub accepted: bool,
    /// The server's version after the push when accepted; its current
    /// version when refused, so the device knows how far behind it is.
    pub version: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// One entry of the server's stream, as a device pulls it.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Change {
    pub seq: i64,
    pub entity_type: String,
    pub entity_id: String,
    pub version: i64,
    pub changed: Vec<String>,
    pub state: Value,
    pub device_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub device_name: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PullPage {
    pub changes: Vec<Change>,
    /// The cursor to pull from next time.
    pub next: i64,
    pub has_more: bool,
}

/// What the engine needs from the network: two calls. Implemented over HTTP
/// in the desktop shell and over a direct function call in tests.
pub trait Transport {
    fn push(&self, items: &[PushItem]) -> Result<Vec<PushResult>>;
    fn pull(&self, after: i64, limit: usize) -> Result<PullPage>;
}
