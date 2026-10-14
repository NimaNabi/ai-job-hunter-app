//! The optional self-hosted sync's per-record view of `aiGenerations`
//! (ADR-051): one record per generation, keyed by its id.

use rusqlite::params;
use serde_json::Value;

use super::quality_report::sanitize_quality_report;
use super::rows::insert_row;
use super::{migrations, AiGenerationRecord, AiGenerationStore};
use crate::data_store::{
    array_records, parse_record, str_field, DataStore, SyncRecord, SyncSection,
};
use crate::error::AppResult;

impl SyncSection for AiGenerationStore {
    fn section(&self) -> &'static str {
        self.key()
    }

    fn schema_version(&self) -> u32 {
        migrations::MIGRATIONS.len() as u32
    }

    fn sync_records(&self) -> AppResult<Vec<SyncRecord>> {
        array_records(self.export(), |row| str_field(row, "id"))
    }

    /// Whole-record replace in one transaction, with the same guards as a
    /// backup restore: the payload comes from another device, so it is as
    /// untrusted as a bundle (`backup.rs`).
    fn upsert_record(&self, id: &str, payload: &Value) -> AppResult<()> {
        let mut rec: AiGenerationRecord = parse_record(payload)?;
        rec.id = id.to_string();
        let quality_report = sanitize_quality_report(rec.quality_report.clone(), "sync");
        let mut guard = self.conn.lock();
        let tx = guard.transaction()?;
        tx.execute("DELETE FROM ai_generations WHERE id = ?1", params![id])?;
        insert_row(&tx, &rec, &quality_report)?;
        tx.commit()?;
        Ok(())
    }

    fn delete_record(&self, id: &str) -> AppResult<()> {
        self.remove(id)
    }
}
