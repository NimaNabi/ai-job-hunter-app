//! Optional self-hosted sync (ADR-051) — the IPC seam over
//! [`crate::sync_scheduler`] and the settings in `sync_state.db`.
//!
//! The token goes straight into the OS keychain and is never returned, logged
//! or included in an error. Every network call runs on the blocking pool
//! (`sync::http` blocks on the shared async client).

use parking_lot::Mutex;
use tauri::{AppHandle, Manager};

use crate::credentials::CredentialStore;
use crate::error::{AppError, AppResult};
use crate::sync::engine::Transport;
use crate::sync::envelope::PROTOCOL_VERSION;
use crate::sync::http::{HttpTransport, TOKEN_REJECTED};
use crate::sync::settings::{self, normalize_server_url, SyncStatus};
use crate::sync::state::SyncStateStore;
use crate::sync_scheduler::{run_now, TOKEN_SLOT};

fn state_store(app: &AppHandle) -> AppResult<tauri::State<'_, SyncStateStore>> {
    app.try_state::<SyncStateStore>()
        .ok_or_else(|| AppError::Storage("sync is unavailable".into()))
}

fn credentials(app: &AppHandle) -> tauri::State<'_, Mutex<CredentialStore>> {
    app.state::<Mutex<CredentialStore>>()
}

/// Settings, the last run's outcome, and whether the URL is unencrypted.
#[tauri::command]
pub async fn sync_status(app: AppHandle) -> AppResult<SyncStatus> {
    settings::status(&*state_store(&app)?)
}

/// Check the server and the token, then save the URL and the token (OS
/// keychain), turn sync on and run once.
///
/// Nothing is saved unless the server answers `/health` with a protocol this
/// app speaks AND accepts the token on an authenticated `GET /changes`
/// (`/health` may be unauthenticated, so it proves nothing about the token).
/// If the first run is still rejected, the previous URL, on/off state and
/// token are put back, so re-entering a wrong token never breaks a setup that
/// worked. Any other first-run failure (unreachable, or a scheduled run
/// already in progress) keeps the new, already-validated settings; it is
/// recorded in the status and the next run retries.
#[tauri::command]
pub async fn sync_configure(
    app: AppHandle,
    server_url: String,
    token: String,
) -> AppResult<SyncStatus> {
    let url = normalize_server_url(&server_url)?;
    let token = token.trim().to_string();
    if token.is_empty() {
        return Err(AppError::Validation("a sync token is required".into()));
    }

    let runtime = tokio::runtime::Handle::current();
    let (probe_url, probe_token) = (url.clone(), token.clone());
    tokio::task::spawn_blocking(move || check_server(&probe_url, &probe_token, runtime))
        .await
        .map_err(|_| AppError::message("sync server check stopped unexpectedly"))??;

    let state = state_store(&app)?;
    let before = settings::status(&state)?;
    let previous_token = crate::credentials::read_credential(TOKEN_SLOT)?;
    credentials(&app).lock().set(TOKEN_SLOT, "sync", &token)?;
    settings::enable(&state, &url)?;

    match run_now(&app).await {
        Err(AppError::Config(m)) if m == TOKEN_REJECTED => {
            match previous_token {
                Some(t) => credentials(&app).lock().set(TOKEN_SLOT, "sync", &t)?,
                None => credentials(&app).lock().remove(TOKEN_SLOT)?,
            }
            settings::restore(&state, before.server_url.as_deref(), before.enabled)?;
            Err(AppError::Config(TOKEN_REJECTED.into()))
        }
        _ => settings::status(&state),
    }
}

/// `/health` reports protocol v1 and the token is accepted.
fn check_server(url: &str, token: &str, runtime: tokio::runtime::Handle) -> AppResult<()> {
    let transport = HttpTransport::new(url, token, runtime);
    let version = transport.health()?;
    if version != PROTOCOL_VERSION {
        return Err(AppError::Validation(format!(
            "sync server speaks protocol {version}, this app speaks {PROTOCOL_VERSION}"
        )));
    }
    transport.pull(None).map(|_| ())
}

/// Turn sync off and remove the token from the keychain. Local data is
/// untouched.
#[tauri::command]
pub async fn sync_disable(app: AppHandle) -> AppResult<SyncStatus> {
    let state = state_store(&app)?;
    settings::disable(&state)?;
    credentials(&app).lock().remove(TOKEN_SLOT)?;
    settings::status(&state)
}

/// Run one sync now ("Sync now").
#[tauri::command]
pub async fn sync_now(app: AppHandle) -> AppResult<SyncStatus> {
    run_now(&app).await
}

#[cfg(test)]
mod tests;
