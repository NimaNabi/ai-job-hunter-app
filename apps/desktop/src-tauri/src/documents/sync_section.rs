//! The optional self-hosted sync's per-record view of `documents` (ADR-051):
//! one record per document, keyed by its id.
//!
//! Unlike the backup export, a record carries no embedding (`vector`,
//! `vectorSpace`) and no `indexed` flag: both describe this device's index,
//! not the document. A document whose text changes is re-embedded by the
//! receiving device, exactly as after an edit made there.

use rusqlite::{params, OptionalExtension};
use serde_json::Value;

use super::{DocumentRecord, DocumentStore};
use crate::data_store::{DataStore, SyncRecord, SyncSection};
use crate::db::ts_to_db;
use crate::error::AppResult;

impl SyncSection for DocumentStore {
    fn section(&self) -> &'static str {
        self.key()
    }

    fn schema_version(&self) -> u32 {
        Self::MIGRATIONS.len() as u32
    }

    fn sync_records(&self) -> AppResult<Vec<SyncRecord>> {
        let mut out = Vec::new();
        for rec in self.list() {
            let mut payload = serde_json::to_value(&rec)?;
            if let Some(obj) = payload.as_object_mut() {
                obj.remove("indexed");
            }
            out.push(SyncRecord {
                id: rec.id,
                payload,
            });
        }
        Ok(out)
    }

    /// Insert or update in one transaction. The local `indexed` flag is kept,
    /// and the document's vector is dropped only when its text changed.
    fn upsert_record(&self, id: &str, payload: &Value) -> AppResult<()> {
        let mut body = payload.clone();
        if let Some(obj) = body.as_object_mut() {
            obj.insert("indexed".into(), Value::Bool(false));
            obj.insert("_id".into(), Value::String(id.to_string()));
        }
        let rec: DocumentRecord = serde_json::from_value(body)?;
        // Same defensive repair as `insert`: a peer on an older build can
        // still hold pre-#955 mojibake.
        let text = crate::extraction::pdf::repair_utf16_mojibake(&rec.text);

        let mut guard = self.conn.lock();
        let tx = guard.transaction()?;
        let old_text: Option<String> = tx
            .query_row(
                "SELECT text FROM documents WHERE id = ?1",
                params![id],
                |r| r.get(0),
            )
            .optional()?;
        tx.execute(
            "INSERT INTO documents
                (id, title, name, locale, text, pages, created_at, indexed, is_default, keywords_json)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 0, ?8, ?9)
             ON CONFLICT(id) DO UPDATE SET
                title = excluded.title, name = excluded.name, locale = excluded.locale,
                text = excluded.text, pages = excluded.pages, created_at = excluded.created_at,
                is_default = excluded.is_default, keywords_json = excluded.keywords_json",
            params![
                id,
                rec.title,
                rec.name,
                rec.locale,
                text.as_ref(),
                rec.pages,
                ts_to_db(rec.created_at),
                i64::from(rec.is_default),
                rec.keywords_json,
            ],
        )?;
        if old_text.as_deref() != Some(text.as_ref()) {
            tx.execute("DELETE FROM vectors WHERE doc_id = ?1", params![id])?;
            tx.execute(
                "UPDATE documents SET indexed = 0 WHERE id = ?1",
                params![id],
            )?;
        }
        if rec.is_default {
            tx.execute("UPDATE documents SET is_default = (id = ?1)", params![id])?;
        }
        tx.commit()?;
        Ok(())
    }

    fn delete_record(&self, id: &str) -> AppResult<()> {
        self.remove(id)
    }
}
