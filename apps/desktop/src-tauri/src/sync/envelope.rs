//! Wire types of the v1 sync protocol (`docs/knowledge/sync-protocol.md`) and
//! the record hash the bundle diff compares.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::error::{AppError, AppResult};

/// The protocol version this client speaks (`GET /health`).
pub const PROTOCOL_VERSION: u32 = 1;

/// One change. The server stores it as an opaque blob.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Envelope {
    pub store: String,
    pub record_id: String,
    /// The writer's sync-run time in ms; orders writes (last sync wins).
    pub rev: u64,
    pub device_id: String,
    pub deleted: bool,
    pub schema_version: u32,
    /// The record as the section exports it; `null` for a tombstone.
    pub payload: Value,
}

impl Envelope {
    /// `(rev, device_id)` — the total order the conflict rule compares.
    pub fn order(&self) -> (u64, &str) {
        (self.rev, self.device_id.as_str())
    }

    /// Structural check of an envelope received from a server. A malformed
    /// envelope rejects its whole page (the cursor does not advance).
    pub fn validate(&self) -> AppResult<()> {
        let bad = |what: &str| Err(AppError::Validation(format!("sync envelope: {what}")));
        if self.store.is_empty() {
            return bad("empty store");
        }
        if self.record_id.is_empty() {
            return bad("empty recordId");
        }
        if self.device_id.is_empty() {
            return bad("empty deviceId");
        }
        if self.deleted != self.payload.is_null() {
            return bad("payload must be null exactly for a tombstone");
        }
        Ok(())
    }
}

/// One `GET /changes` page.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Page {
    pub changes: Vec<Envelope>,
    pub cursor: String,
    pub has_more: bool,
}

/// Content hash of a record payload. `serde_json` keeps object keys sorted,
/// so two devices exporting the same record produce the same hash.
pub fn record_hash(payload: &Value) -> String {
    let bytes = serde_json::to_vec(payload).unwrap_or_default();
    let digest = Sha256::digest(&bytes);
    digest.iter().map(|b| format!("{b:02x}")).collect()
}

/// The sync-state scope: one per (server URL, token), because the token picks
/// the namespace. Hashed so the token itself is never stored or logged.
pub fn scope_key(server_url: &str, token: &str) -> String {
    record_hash(&serde_json::json!([
        server_url.trim_end_matches('/'),
        token
    ]))
}
