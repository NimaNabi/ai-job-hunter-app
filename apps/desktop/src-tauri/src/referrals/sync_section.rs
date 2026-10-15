//! The optional self-hosted sync's per-record view of `referrals` (ADR-051):
//! one record per referral, keyed by its id.

use rusqlite::params;
use serde_json::Value;

use super::{insert_conn, ReferralContact, ReferralStore};
use crate::data_store::{
    array_records, parse_record, str_field, DataStore, SyncRecord, SyncSection,
};
use crate::error::AppResult;

impl SyncSection for ReferralStore {
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
        let mut rec: ReferralContact = parse_record(payload)?;
        rec.id = id.to_string();
        insert_conn(&self.conn.lock(), &rec)
    }

    fn delete_record(&self, id: &str) -> AppResult<()> {
        let conn = self.conn.lock();
        conn.execute("DELETE FROM referrals WHERE id = ?1", params![id])?;
        Ok(())
    }
}
