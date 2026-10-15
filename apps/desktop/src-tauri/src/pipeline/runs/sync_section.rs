//! The optional self-hosted sync's per-record view of `pipelineRuns`
//! (ADR-051). The bundle section is one object of two arrays; here each run
//! and each event is its own record, keyed `["run", id]` or
//! `["event", runId, seq]`, so two devices' run histories union.

use rusqlite::params;
use serde_json::Value;

use super::import_export::{check_event, check_run, insert_event_conn, insert_run_conn};
use super::model::{RunEventRow, RunRow};
use super::store::PipelineRunStore;
use crate::data_store::{composite_id, parse_record, split_id, DataStore, SyncRecord, SyncSection};
use crate::error::{AppError, AppResult};

/// A record id, parsed.
enum Key {
    Run(String),
    Event(String, u32),
}

fn key_of(id: &str) -> AppResult<Key> {
    let parts: Vec<String> = serde_json::from_str(id)?;
    match parts.first().map(String::as_str) {
        Some("run") => Ok(Key::Run(split_id(id, 2)?.remove(1))),
        Some("event") => {
            let p = split_id(id, 3)?;
            let seq = p[2]
                .parse()
                .map_err(|_| AppError::Validation("pipeline event id: bad seq".into()))?;
            Ok(Key::Event(p[1].clone(), seq))
        }
        _ => Err(AppError::Validation(
            "pipeline run record id: unknown kind".into(),
        )),
    }
}

impl SyncSection for PipelineRunStore {
    fn section(&self) -> &'static str {
        self.key()
    }

    fn schema_version(&self) -> u32 {
        Self::MIGRATIONS.len() as u32
    }

    fn sync_records(&self) -> AppResult<Vec<SyncRecord>> {
        let mut out = Vec::new();
        for run in self.all_runs() {
            out.push(SyncRecord {
                id: composite_id(&["run", &run.id]),
                payload: serde_json::to_value(&run)?,
            });
        }
        for ev in self.all_events() {
            out.push(SyncRecord {
                id: composite_id(&["event", &ev.run_id, &ev.seq.to_string()]),
                payload: serde_json::to_value(&ev)?,
            });
        }
        Ok(out)
    }

    /// One `INSERT OR REPLACE`, with the same caps as a backup restore.
    fn upsert_record(&self, id: &str, payload: &Value) -> AppResult<()> {
        let conn = self.conn.lock();
        match key_of(id)? {
            Key::Run(run_id) => {
                let run = RunRow {
                    id: run_id,
                    ..parse_record(payload)?
                };
                check_run(&run)?;
                insert_run_conn(&conn, &run)
            }
            Key::Event(run_id, seq) => {
                let event = RunEventRow {
                    run_id,
                    seq,
                    ..parse_record(payload)?
                };
                check_event(&event)?;
                insert_event_conn(&conn, &event)
            }
        }
    }

    fn delete_record(&self, id: &str) -> AppResult<()> {
        let mut guard = self.conn.lock();
        match key_of(id)? {
            Key::Run(run_id) => {
                let tx = guard.transaction()?;
                tx.execute(
                    "DELETE FROM pipeline_run_events WHERE run_id = ?1",
                    params![run_id],
                )?;
                tx.execute("DELETE FROM pipeline_runs WHERE id = ?1", params![run_id])?;
                tx.commit()?;
            }
            Key::Event(run_id, seq) => {
                guard.execute(
                    "DELETE FROM pipeline_run_events WHERE run_id = ?1 AND seq = ?2",
                    params![run_id, seq],
                )?;
            }
        }
        Ok(())
    }
}
