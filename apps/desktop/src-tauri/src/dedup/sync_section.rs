//! The optional self-hosted sync's per-record view of `dedupTombstones`
//! (ADR-051): one record per "not a duplicate" verdict, keyed by its
//! canonical `(keyA, keyB)` pair.

use rusqlite::params;
use serde_json::Value;

use super::{insert_conn, DedupStore, TombstoneRow};
use crate::data_store::{
    array_records, composite_id, parse_record, split_id, str_field, DataStore, SyncRecord,
    SyncSection,
};
use crate::error::AppResult;

impl SyncSection for DedupStore {
    fn section(&self) -> &'static str {
        self.key()
    }

    fn schema_version(&self) -> u32 {
        Self::MIGRATIONS.len() as u32
    }

    fn sync_records(&self) -> AppResult<Vec<SyncRecord>> {
        array_records(self.export(), |row| {
            Some(composite_id(&[
                &str_field(row, "keyA")?,
                &str_field(row, "keyB")?,
            ]))
        })
    }

    fn upsert_record(&self, id: &str, payload: &Value) -> AppResult<()> {
        let key = split_id(id, 2)?;
        let row = TombstoneRow {
            key_a: key[0].clone(),
            key_b: key[1].clone(),
            ..parse_record(payload)?
        };
        let mut guard = self.conn.lock();
        let tx = guard.transaction()?;
        delete_conn(&tx, &key)?;
        insert_conn(&tx, &row)?;
        tx.commit()?;
        Ok(())
    }

    fn delete_record(&self, id: &str) -> AppResult<()> {
        delete_conn(&self.conn.lock(), &split_id(id, 2)?)
    }
}

fn delete_conn(conn: &rusqlite::Connection, key: &[String]) -> AppResult<()> {
    conn.execute(
        "DELETE FROM dedup_tombstones WHERE key_a = ?1 AND key_b = ?2",
        params![key[0], key[1]],
    )?;
    Ok(())
}
