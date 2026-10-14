//! The optional self-hosted sync's per-record view of `discoveredCompanies`
//! (ADR-051): one record per harvested ATS company, keyed by
//! its `(atsKind, slug)` pair.

use rusqlite::params;
use serde_json::Value;

use super::{insert_conn, DiscoveredCompanyStore, DiscoveredRow};
use crate::data_store::{
    array_records, composite_id, parse_record, split_id, str_field, DataStore, SyncRecord,
    SyncSection,
};
use crate::error::AppResult;

impl SyncSection for DiscoveredCompanyStore {
    fn section(&self) -> &'static str {
        self.key()
    }

    fn schema_version(&self) -> u32 {
        Self::MIGRATIONS.len() as u32
    }

    fn sync_records(&self) -> AppResult<Vec<SyncRecord>> {
        array_records(self.export(), |row| {
            Some(composite_id(&[
                &str_field(row, "atsKind")?,
                &str_field(row, "slug")?,
            ]))
        })
    }

    fn upsert_record(&self, id: &str, payload: &Value) -> AppResult<()> {
        let key = split_id(id, 2)?;
        let row = DiscoveredRow {
            ats_kind: key[0].clone(),
            slug: key[1].clone(),
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
        "DELETE FROM discovered_companies WHERE ats_kind = ?1 AND slug = ?2",
        params![key[0], key[1]],
    )?;
    Ok(())
}
