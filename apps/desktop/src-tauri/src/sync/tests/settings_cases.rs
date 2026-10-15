//! Sync settings in `sync_state.db` and the server URL check.

use super::*;
use crate::sync::settings::{self, normalize_server_url};

#[test]
fn a_server_url_must_be_http_or_https_without_credentials() {
    assert_eq!(
        normalize_server_url(" https://sync.example.org/ ").unwrap(),
        "https://sync.example.org"
    );
    assert_eq!(
        normalize_server_url("http://192.168.1.20:8787").unwrap(),
        "http://192.168.1.20:8787"
    );
    assert!(normalize_server_url("ftp://sync.example.org").is_err());
    assert!(normalize_server_url("sync.example.org").is_err());
    assert!(normalize_server_url("https://user:pass@sync.example.org").is_err());
}

#[test]
fn status_reflects_enable_runs_and_disable_and_flags_http() {
    let dir = TempDir::new().unwrap();
    let state = SyncStateStore::open(dir.path()).unwrap();
    assert_eq!(settings::status(&state).unwrap(), Default::default());

    settings::enable(&state, "http://192.168.1.20:8787").unwrap();
    let report = SyncReport {
        pulled: 3,
        applied: 2,
        held: 0,
        pushed: 1,
    };
    settings::record_success(&state, &report, 1234).unwrap();
    let s = settings::status(&state).unwrap();
    assert!(s.enabled && s.insecure);
    assert_eq!(s.last_sync_at, Some(1234));
    assert_eq!(s.last_report.unwrap().applied, 2);

    settings::record_failure(&state, "sync server unreachable").unwrap();
    let s = settings::status(&state).unwrap();
    assert_eq!(
        s.last_sync_at,
        Some(1234),
        "a failure keeps the last sync time"
    );
    assert_eq!(s.last_error.as_deref(), Some("sync server unreachable"));

    settings::disable(&state).unwrap();
    assert!(!settings::status(&state).unwrap().enabled);
}

#[test]
fn a_reset_forgets_everything_but_the_device_id() {
    let dir = TempDir::new().unwrap();
    let state = SyncStateStore::open(dir.path()).unwrap();
    let id = state.device_id().unwrap();
    settings::enable(&state, "https://sync.example.org").unwrap();
    state.set_cursor("scope", "c1").unwrap();
    state.reset().unwrap();
    assert_eq!(state.device_id().unwrap(), id);
    assert!(state.cursor("scope").unwrap().is_none());
    assert!(!settings::status(&state).unwrap().enabled);
}
