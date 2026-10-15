//! The optional self-hosted sync's per-record view of `autopilots` (ADR-051):
//! one record per autopilot, keyed by its id, in the backup shape (so with
//! its found jobs, and without the local board-health verdict).
//!
//! Known limit: an autopilot is one whole record, so if both devices run the
//! same autopilot between syncs, the later sync's found-jobs list wins.

use serde_json::Value;

use super::{strip_board_health, Autopilot, AutopilotStore};
use crate::data_store::{
    array_records, parse_record, str_field, DataStore, SyncRecord, SyncSection,
};
use crate::error::AppResult;

impl SyncSection for AutopilotStore {
    fn section(&self) -> &'static str {
        self.key()
    }

    /// A JSON-file store without migrations: its record shape is version 1.
    fn schema_version(&self) -> u32 {
        1
    }

    fn sync_records(&self) -> AppResult<Vec<SyncRecord>> {
        array_records(self.export(), |row| str_field(row, "_id"))
    }

    fn upsert_record(&self, id: &str, payload: &Value) -> AppResult<()> {
        let mut ap: Autopilot = parse_record(payload)?;
        ap.id = id.to_string();
        strip_board_health(std::iter::once(&mut ap));
        let mut items: Vec<Autopilot> = self.list().into_iter().filter(|a| a.id != id).collect();
        items.push(ap);
        self.replace_all(items);
        Ok(())
    }

    fn delete_record(&self, id: &str) -> AppResult<()> {
        self.remove(id);
        Ok(())
    }
}
