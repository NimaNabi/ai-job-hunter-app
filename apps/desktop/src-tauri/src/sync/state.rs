//! `sync_state.db` — the device's own sync bookkeeping. Machine-local by
//! design: it is NOT a `DataStore`, never enters the export bundle and never
//! syncs (ADR-051).
//!
//! - `meta`: this install's random device id.
//! - `scopes`: one row per (server URL, token) scope with its saved cursor;
//!   no row means that scope has never synced (a first sync).
//! - `baseline`: per scope, the last known state of each record — its hash
//!   and the `(rev, deviceId, schemaVersion)` of the write that produced it,
//!   plus a `deleted` flag that makes tombstones final.
//! - `held`: per scope, envelopes from a newer schema kept until the app is
//!   updated (one per record, the most recently received).

use std::path::Path;

use parking_lot::Mutex;
use rusqlite::{params, Connection, OptionalExtension};

use super::envelope::Envelope;
use crate::db::{run_migrations, ts_from_db, ts_to_db, Migration};
use crate::error::AppResult;

/// The last known state of one record in one scope.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BaselineEntry {
    pub hash: String,
    pub rev: u64,
    pub device_id: String,
    pub schema_version: u32,
    pub deleted: bool,
}

pub struct SyncStateStore {
    conn: Mutex<Connection>,
}

impl SyncStateStore {
    const MIGRATIONS: &'static [Migration] = &[Migration {
        name: "create_sync_state",
        up: |conn| {
            conn.execute_batch(
                "CREATE TABLE IF NOT EXISTS meta (
                    key   TEXT PRIMARY KEY,
                    value TEXT NOT NULL
                );
                CREATE TABLE IF NOT EXISTS scopes (
                    scope  TEXT PRIMARY KEY,
                    cursor TEXT
                );
                CREATE TABLE IF NOT EXISTS baseline (
                    scope          TEXT NOT NULL,
                    store          TEXT NOT NULL,
                    record_id      TEXT NOT NULL,
                    hash           TEXT NOT NULL,
                    rev            INTEGER NOT NULL,
                    device_id      TEXT NOT NULL,
                    schema_version INTEGER NOT NULL,
                    deleted        INTEGER NOT NULL DEFAULT 0,
                    PRIMARY KEY (scope, store, record_id)
                );
                CREATE TABLE IF NOT EXISTS held (
                    scope     TEXT NOT NULL,
                    store     TEXT NOT NULL,
                    record_id TEXT NOT NULL,
                    envelope  TEXT NOT NULL,
                    PRIMARY KEY (scope, store, record_id)
                );",
            )
        },
    }];

    pub fn open(data_dir: &Path) -> AppResult<Self> {
        std::fs::create_dir_all(data_dir)?;
        let mut conn = crate::db::open(&data_dir.join("sync_state.db"))?;
        run_migrations(&mut conn, Self::MIGRATIONS)?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    /// This install's device id, created on first use.
    pub fn device_id(&self) -> AppResult<String> {
        let conn = self.conn.lock();
        let existing: Option<String> = conn
            .query_row("SELECT value FROM meta WHERE key = 'device_id'", [], |r| {
                r.get(0)
            })
            .optional()?;
        if let Some(id) = existing {
            return Ok(id);
        }
        let id = uuid::Uuid::new_v4().to_string();
        conn.execute(
            "INSERT INTO meta (key, value) VALUES ('device_id', ?1)",
            params![id],
        )?;
        Ok(id)
    }

    /// The saved cursor, `None` before the first page was acknowledged.
    pub fn cursor(&self, scope: &str) -> AppResult<Option<String>> {
        let conn = self.conn.lock();
        Ok(conn
            .query_row(
                "SELECT cursor FROM scopes WHERE scope = ?1",
                params![scope],
                |r| r.get::<_, Option<String>>(0),
            )
            .optional()?
            .flatten())
    }

    pub fn set_cursor(&self, scope: &str, cursor: &str) -> AppResult<()> {
        let conn = self.conn.lock();
        conn.execute(
            "INSERT INTO scopes (scope, cursor) VALUES (?1, ?2)
             ON CONFLICT(scope) DO UPDATE SET cursor = excluded.cursor",
            params![scope, cursor],
        )?;
        Ok(())
    }

    /// Every baseline row of a scope, keyed by `(store, record_id)`.
    pub fn baseline(&self, scope: &str) -> AppResult<Vec<(String, String, BaselineEntry)>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            "SELECT store, record_id, hash, rev, device_id, schema_version, deleted
             FROM baseline WHERE scope = ?1",
        )?;
        let rows = stmt.query_map(params![scope], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                BaselineEntry {
                    hash: r.get(2)?,
                    rev: ts_from_db(r.get::<_, i64>(3)?),
                    device_id: r.get(4)?,
                    schema_version: r.get(5)?,
                    deleted: r.get::<_, i64>(6)? != 0,
                },
            ))
        })?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    pub fn put_baseline(
        &self,
        scope: &str,
        store: &str,
        record_id: &str,
        entry: &BaselineEntry,
    ) -> AppResult<()> {
        let conn = self.conn.lock();
        conn.execute(
            "INSERT INTO baseline
                (scope, store, record_id, hash, rev, device_id, schema_version, deleted)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
             ON CONFLICT(scope, store, record_id) DO UPDATE SET
                hash = excluded.hash, rev = excluded.rev, device_id = excluded.device_id,
                schema_version = excluded.schema_version, deleted = excluded.deleted",
            params![
                scope,
                store,
                record_id,
                entry.hash,
                ts_to_db(entry.rev),
                entry.device_id,
                entry.schema_version,
                i64::from(entry.deleted),
            ],
        )?;
        Ok(())
    }

    /// Keep `env` (newer schema) for later, replacing an older held one.
    pub fn hold(&self, scope: &str, env: &Envelope) -> AppResult<()> {
        let json = serde_json::to_string(env)?;
        let conn = self.conn.lock();
        conn.execute(
            "INSERT INTO held (scope, store, record_id, envelope) VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(scope, store, record_id) DO UPDATE SET envelope = excluded.envelope",
            params![scope, env.store, env.record_id, json],
        )?;
        Ok(())
    }

    pub fn held(&self, scope: &str) -> AppResult<Vec<Envelope>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare("SELECT envelope FROM held WHERE scope = ?1")?;
        let rows = stmt.query_map(params![scope], |r| r.get::<_, String>(0))?;
        let mut out = Vec::new();
        for json in rows {
            out.push(serde_json::from_str(&json?)?);
        }
        Ok(out)
    }

    pub fn release(&self, scope: &str, store: &str, record_id: &str) -> AppResult<()> {
        let conn = self.conn.lock();
        conn.execute(
            "DELETE FROM held WHERE scope = ?1 AND store = ?2 AND record_id = ?3",
            params![scope, store, record_id],
        )?;
        Ok(())
    }
}
