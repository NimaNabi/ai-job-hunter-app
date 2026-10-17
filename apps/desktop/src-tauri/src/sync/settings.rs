//! Sync settings and the last run's outcome, kept in `sync_state.db`'s
//! machine-local `meta` table (never in the export bundle, never synced).
//! The token is not here: it lives in the OS keychain (ADR 0005 class 9).

use serde::Serialize;

use super::engine::SyncReport;
use super::state::SyncStateStore;
use crate::error::{AppError, AppResult};

const ENABLED: &str = "enabled";
const SERVER_URL: &str = "server_url";
const LAST_SYNC_AT: &str = "last_sync_at";
const LAST_ERROR: &str = "last_error";
const LAST_REPORT: &str = "last_report";

/// What the Settings card shows. Never carries the token. An unset field is
/// left out (the TS contract declares it optional), never sent as `null`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncStatus {
    pub enabled: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub server_url: Option<String>,
    /// The URL is `http://`: traffic is unencrypted unless the link itself is
    /// private (protocol "Authentication and transport").
    pub insecure: bool,
    /// Epoch ms of the last successful run.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_sync_at: Option<u64>,
    /// The last run's error, cleared by a successful run.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_report: Option<ReportView>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReportView {
    pub pulled: usize,
    pub applied: usize,
    pub held: usize,
    pub pushed: usize,
}

impl From<&SyncReport> for ReportView {
    fn from(r: &SyncReport) -> Self {
        Self {
            pulled: r.pulled,
            applied: r.applied,
            held: r.held,
            pushed: r.pushed,
        }
    }
}

/// Check a user-typed server URL: `http(s)://host[/path]`, with no
/// credentials, query or fragment in it. Returns it without a trailing slash.
pub fn normalize_server_url(raw: &str) -> AppResult<String> {
    let trimmed = raw.trim().trim_end_matches('/');
    let url = reqwest::Url::parse(trimmed)
        .map_err(|_| AppError::Validation("sync server URL is not a valid URL".into()))?;
    if url.scheme() != "https" && url.scheme() != "http" {
        return Err(AppError::Validation(
            "sync server URL must start with https:// or http://".into(),
        ));
    }
    if url.host_str().is_none_or(str::is_empty) {
        return Err(AppError::Validation("sync server URL has no host".into()));
    }
    if url.query().is_some() || url.fragment().is_some() {
        // Endpoint paths are appended to the base URL, so `?x=1` or `#a`
        // would end up in front of `/health` and `/changes`.
        return Err(AppError::Validation(
            "sync server URL must not contain ? or #".into(),
        ));
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err(AppError::Validation(
            "put the token in the token field, not in the URL".into(),
        ));
    }
    // The parsed form, so the scheme and host are lowercase (`HTTP://` is
    // stored, and flagged insecure, as `http://`).
    Ok(url.as_str().trim_end_matches('/').to_string())
}

/// The stored URL is plain `http://`.
fn is_insecure(server_url: &str) -> bool {
    reqwest::Url::parse(server_url).is_ok_and(|u| u.scheme() == "http")
}

pub fn status(state: &SyncStateStore) -> AppResult<SyncStatus> {
    let server_url = state.meta(SERVER_URL)?;
    Ok(SyncStatus {
        enabled: state.meta(ENABLED)?.as_deref() == Some("1"),
        insecure: server_url.as_deref().is_some_and(is_insecure),
        server_url,
        last_sync_at: state.meta(LAST_SYNC_AT)?.and_then(|v| v.parse().ok()),
        last_error: state.meta(LAST_ERROR)?,
        last_report: state
            .meta(LAST_REPORT)?
            .and_then(|v| serde_json::from_str(&v).ok()),
    })
}

/// Turn sync on for `server_url` (already normalized).
pub fn enable(state: &SyncStateStore, server_url: &str) -> AppResult<()> {
    state.set_meta(SERVER_URL, Some(server_url))?;
    state.set_meta(LAST_ERROR, None)?;
    state.set_meta(ENABLED, Some("1"))
}

/// Put back an earlier URL, on/off state and last error (a configure whose
/// first run was rejected must not leave the setup that worked showing the
/// rejected attempt's error).
pub fn restore(
    state: &SyncStateStore,
    server_url: Option<&str>,
    enabled: bool,
    last_error: Option<&str>,
) -> AppResult<()> {
    state.set_meta(SERVER_URL, server_url)?;
    state.set_meta(LAST_ERROR, last_error)?;
    state.set_meta(ENABLED, enabled.then_some("1"))
}

/// Turn sync off. The URL stays so the card can offer it again; the baseline
/// stays too, keyed by URL + token, so turning the same pair back on resumes.
pub fn disable(state: &SyncStateStore) -> AppResult<()> {
    state.set_meta(ENABLED, None)
}

pub fn record_success(state: &SyncStateStore, report: &SyncReport, at: u64) -> AppResult<()> {
    let view = serde_json::to_string(&ReportView::from(report))?;
    state.set_meta(LAST_REPORT, Some(&view))?;
    state.set_meta(LAST_SYNC_AT, Some(&at.to_string()))?;
    state.set_meta(LAST_ERROR, None)
}

/// Keep the error for the card. A run that fails keeps the last-synced time
/// (protocol "Failure handling").
pub fn record_failure(state: &SyncStateStore, error: &str) -> AppResult<()> {
    state.set_meta(LAST_ERROR, Some(error))
}
