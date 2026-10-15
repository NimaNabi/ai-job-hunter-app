//! The optional self-hosted sync's per-record view of `interactions`
//! (ADR-051): one record per `(jobId, interactionType)`, the store's own key.
//!
//! The store lives behind a `Mutex` in app state, so the section is a borrow
//! of that mutex rather than the store itself.

use parking_lot::Mutex;
use serde_json::Value;

use super::{InteractionRecord, InteractionStore};
use crate::data_store::{composite_id, parse_record, split_id, SyncRecord, SyncSection};
use crate::error::AppResult;

/// The `interactions` bundle section over the app's `Mutex<InteractionStore>`.
pub struct InteractionsSection<'a>(pub &'a Mutex<InteractionStore>);

impl SyncSection for InteractionsSection<'_> {
    fn section(&self) -> &'static str {
        "interactions"
    }

    /// A JSON-file store without migrations: its record shape is version 1.
    fn schema_version(&self) -> u32 {
        1
    }

    fn sync_records(&self) -> AppResult<Vec<SyncRecord>> {
        self.0
            .lock()
            .export_all()
            .into_iter()
            .map(|rec| {
                Ok(SyncRecord {
                    id: composite_id(&[&rec.job_id, &rec.interaction_type]),
                    payload: serde_json::to_value(&rec)?,
                })
            })
            .collect()
    }

    fn upsert_record(&self, id: &str, payload: &Value) -> AppResult<()> {
        let key = split_id(id, 2)?;
        let rec = InteractionRecord {
            job_id: key[0].clone(),
            interaction_type: key[1].clone(),
            ..parse_record(payload)?
        };
        self.0.lock().upsert(rec);
        Ok(())
    }

    fn delete_record(&self, id: &str) -> AppResult<()> {
        let key = split_id(id, 2)?;
        self.0.lock().remove(&key[0], &key[1]);
        Ok(())
    }
}
