//! The optional self-hosted sync's per-record view of `applications`
//! (ADR-051): one record per application, keyed by its id. The status history
//! is its own section (`status_events_sync`), so history from two devices
//! unions instead of being overwritten with the application.

use serde_json::Value;

use super::{migrations, Application, ApplicationStore};
use crate::data_store::{
    array_records, parse_record, str_field, DataStore, SyncRecord, SyncSection,
};
use crate::error::AppResult;

impl SyncSection for ApplicationStore {
    fn section(&self) -> &'static str {
        self.key()
    }

    fn schema_version(&self) -> u32 {
        migrations::MIGRATIONS.len() as u32
    }

    fn sync_records(&self) -> AppResult<Vec<SyncRecord>> {
        array_records(self.export(), |row| str_field(row, "id"))
    }

    /// One upsert statement (`write_row_conn`), so it is atomic on its own.
    /// Unlike a backup restore it seeds no synthetic status event: the real
    /// history arrives as `applicationStatusEvents` records.
    fn upsert_record(&self, id: &str, payload: &Value) -> AppResult<()> {
        let mut app = parse_record::<Application>(payload)?.canonicalize_contact();
        app.id = id.to_string();
        Self::write_row_conn(&self.conn.lock(), &app)
    }

    /// The application and its history, as a delete in the app does.
    fn delete_record(&self, id: &str) -> AppResult<()> {
        self.delete(id, true)
    }
}
