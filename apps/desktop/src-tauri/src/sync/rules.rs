//! The conflict and schema rules of `docs/knowledge/sync-protocol.md`, as pure
//! functions: no store, no I/O, so every case is unit-tested directly.

use super::envelope::Envelope;
use super::state::BaselineEntry;

/// What to do with one pulled envelope.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decision {
    /// Write the payload over the local record.
    Upsert,
    /// Delete the local record and remember the tombstone.
    Delete,
    /// Newer schema than this app understands: keep it, apply after an update.
    Hold,
    /// The local state already wins (or the envelope is an echo).
    Ignore,
}

/// The local side of the comparison for one record.
#[derive(Debug, Clone, Copy)]
pub struct Local<'a> {
    /// The record's last known state in this scope, if any.
    pub baseline: Option<&'a BaselineEntry>,
    /// The record's current hash in the store, `None` if it does not exist.
    pub current_hash: Option<&'a str>,
    /// This store's schema version on this device.
    pub schema_version: u32,
    /// This sync run's rev and this device's id.
    pub run_rev: u64,
    pub device_id: &'a str,
}

pub fn decide(env: &Envelope, local: &Local<'_>, known_store: bool) -> Decision {
    if !known_store || env.schema_version > local.schema_version {
        return Decision::Hold;
    }
    // Never synced from this device: adopt the server's state. A fresh install
    // (whose settings sections hold only defaults) must not overwrite the
    // other device; its local-only records are still pushed afterwards.
    let Some(b) = local.baseline else {
        return apply(env);
    };
    if b.deleted {
        // Tombstones are final.
        return Decision::Ignore;
    }
    let changed = match local.current_hash {
        Some(h) => b.hash != h,
        None => true,
    };
    let (rival_order, rival_schema) = if changed {
        ((local.run_rev, local.device_id), local.schema_version)
    } else {
        ((b.rev, b.device_id.as_str()), b.schema_version)
    };
    if env.order() <= rival_order {
        return Decision::Ignore;
    }
    // Never let an older-schema payload replace a newer-schema record.
    if !env.deleted && env.schema_version < rival_schema {
        return Decision::Ignore;
    }
    apply(env)
}

fn apply(env: &Envelope) -> Decision {
    if env.deleted {
        Decision::Delete
    } else {
        Decision::Upsert
    }
}
