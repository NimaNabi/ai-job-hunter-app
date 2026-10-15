//! The optional self-hosted sync's per-record view of `spend`, the AI usage
//! ledger (ADR-051): one record per call, keyed by its id.

use rusqlite::params;
use serde_json::Value;

use super::{insert_conn, SpendRow, SpendStore};
use crate::data_store::{
    array_records, parse_record, str_field, DataStore, SyncRecord, SyncSection,
};
use crate::error::AppResult;

impl SyncSection for SpendStore {
    fn section(&self) -> &'static str {
        self.key()
    }

    fn schema_version(&self) -> u32 {
        Self::MIGRATIONS.len() as u32
    }

    fn sync_records(&self) -> AppResult<Vec<SyncRecord>> {
        array_records(self.export(), |row| str_field(row, "id"))
    }

    fn upsert_record(&self, id: &str, payload: &Value) -> AppResult<()> {
        let mut rec: SpendRow = parse_record(payload)?;
        rec.id = id.to_string();
        let mut guard = self.conn.lock();
        let tx = guard.transaction()?;
        tx.execute("DELETE FROM ai_spend WHERE id = ?1", params![id])?;
        insert_conn(&tx, &rec)?;
        tx.commit()?;
        Ok(())
    }

    fn delete_record(&self, id: &str) -> AppResult<()> {
        let conn = self.conn.lock();
        conn.execute("DELETE FROM ai_spend WHERE id = ?1", params![id])?;
        Ok(())
    }
}
