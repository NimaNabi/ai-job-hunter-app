//! `applicationStatusEvents` — the status history of every
//! [`super::Application`] as its own export-bundle section (#1402).
//!
//! `ApplicationStore`'s own [`DataStore`] impl (`super::backup`) exports only
//! the applications and, on import, seeds one synthetic `"imported"` event per
//! application. On its own that drops the real history on every
//! backup/restore. This section carries the history verbatim so a restore
//! puts it back, and it is the record shape the optional self-hosted sync
//! (ADR-051) diffs: one record per event, never nested inside the
//! application, so history from two devices unions instead of overwriting.
//!
//! A newtype over the SAME store — status history lives in the
//! `status_events` table of the applications database — so `data_import`
//! routes it through its usual `import_into` closure. It is routed AFTER
//! `"applications"`: that import seeds the synthetic events, and this one
//! replaces them with the real history. A bundle without this section (any
//! backup written before it existed) restores exactly as before.

use rusqlite::params;
use serde::{Deserialize, Serialize};

use crate::data_store::DataStore;
use crate::db::{ts_from_db, ts_to_db};
use crate::error::{AppError, AppResult};

use super::ApplicationStore;

/// One status-history row as it appears in the bundle. Unlike
/// [`super::status_events::StatusEvent`] it carries no `eventId`: that is the
/// row's SQLite `rowid`, which only identifies the row on the device that
/// wrote it, so a restore lets SQLite assign fresh ones.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct StatusEventRecord {
    pub application_id: String,
    #[serde(default)]
    pub from_status: String,
    pub to_status: String,
    pub at: u64,
    #[serde(default)]
    pub note: String,
    pub source: String,
    pub confirmed: bool,
}

/// The `applicationStatusEvents` bundle section over an [`ApplicationStore`].
pub struct ApplicationStatusEvents<'a>(pub &'a ApplicationStore);

impl ApplicationStatusEvents<'_> {
    fn records(&self) -> AppResult<Vec<StatusEventRecord>> {
        let conn = self.0.conn.lock();
        let mut stmt = conn.prepare(
            "SELECT application_id, from_status, to_status, at, note, source, confirmed
             FROM status_events ORDER BY application_id ASC, at ASC, rowid ASC",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(StatusEventRecord {
                application_id: row.get(0)?,
                from_status: row.get(1)?,
                to_status: row.get(2)?,
                at: ts_from_db(row.get::<_, i64>(3)?),
                note: row.get(4)?,
                source: row.get(5)?,
                confirmed: row.get::<_, i64>(6)? != 0,
            })
        })?;
        Ok(rows.filter_map(Result::ok).collect())
    }
}

impl DataStore for ApplicationStatusEvents<'_> {
    fn key(&self) -> &'static str {
        "applicationStatusEvents"
    }

    fn export(&self) -> serde_json::Value {
        serde_json::json!(self.records().unwrap_or_default())
    }

    /// REPLACE semantics, like every other section: every row in
    /// `status_events` is replaced by the bundle's rows, inside one
    /// transaction, after every record has been parsed. An event whose
    /// application is not in the store is skipped rather than kept as an
    /// orphan. Returns the number of events written.
    fn import(&self, data: &serde_json::Value) -> AppResult<usize> {
        let items = data
            .as_array()
            .ok_or_else(|| AppError::Parse("applicationStatusEvents: expected an array".into()))?;
        let events: Vec<StatusEventRecord> = items
            .iter()
            .map(|item| serde_json::from_value(item.clone()).map_err(AppError::from))
            .collect::<AppResult<_>>()?;

        let mut guard = self.0.conn.lock();
        let tx = guard.transaction()?;
        tx.execute("DELETE FROM status_events", [])?;
        let mut written = 0;
        for ev in &events {
            written += tx.execute(
                "INSERT INTO status_events
                    (application_id, from_status, to_status, at, note, source, confirmed)
                 SELECT ?1, ?2, ?3, ?4, ?5, ?6, ?7
                 WHERE EXISTS (SELECT 1 FROM applications WHERE id = ?1)",
                params![
                    ev.application_id,
                    ev.from_status,
                    ev.to_status,
                    ts_to_db(ev.at),
                    ev.note,
                    ev.source,
                    i64::from(ev.confirmed),
                ],
            )?;
        }
        tx.commit()?;
        Ok(written)
    }
}
