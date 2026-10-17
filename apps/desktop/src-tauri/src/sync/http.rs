//! The v1 protocol over HTTP (`docs/knowledge/sync-protocol.md`): the
//! [`Transport`] the engine uses against a real, user-run server (ADR 0005
//! class 9). Every request goes through the shared client in `net::http`.
//!
//! The engine is synchronous (it drives the stores' own locks), so it runs on
//! a blocking thread and this transport blocks on the async client through
//! the runtime handle it was built with.
//!
//! The token is sent only in the `Authorization` header and never appears in
//! an error or a log line.

use std::time::Duration;

use reqwest::StatusCode;
use serde::Deserialize;
use serde_json::json;
use tokio::runtime::Handle;

use super::engine::Transport;
use super::envelope::{Envelope, Page};
use crate::error::{AppError, AppResult};

/// Per-request timeout. A page or a batch is at most a few MB.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(60);
/// Largest response body accepted: a page is bounded by the server, this
/// bounds a misbehaving one.
const MAX_BODY_BYTES: usize = 64 * 1024 * 1024;
/// Envelopes per `POST /changes`; a `413` halves the batch.
pub const PUSH_BATCH: usize = 500;

/// The error a `401` becomes: the user has to fix the token, so a run that
/// gets it stops instead of retrying.
pub const TOKEN_REJECTED: &str = "sync server rejected the token";

pub struct HttpTransport {
    base: String,
    token: String,
    runtime: Handle,
}

impl HttpTransport {
    /// `base` is the server's base URL; endpoint paths are appended to it.
    pub fn new(base: &str, token: &str, runtime: Handle) -> Self {
        Self {
            base: base.trim_end_matches('/').to_string(),
            token: token.to_string(),
            runtime,
        }
    }

    fn url(&self, path: &str) -> String {
        format!("{}{path}", self.base)
    }

    fn send(&self, req: reqwest::RequestBuilder) -> AppResult<serde_json::Value> {
        self.runtime.block_on(async {
            let resp = req
                .bearer_auth(&self.token)
                .timeout(REQUEST_TIMEOUT)
                .send()
                .await
                .map_err(|e| {
                    AppError::Network(format!("sync server unreachable: {}", e.without_url()))
                })?;
            check_status(resp.status())?;
            crate::net::http::read_json_capped(resp, MAX_BODY_BYTES).await
        })
    }

    fn push_batch(&self, device_id: &str, changes: &[Envelope]) -> AppResult<usize> {
        let req = crate::net::http::shared()
            .post(self.url("/changes"))
            .json(&json!({ "deviceId": device_id, "changes": changes }));
        match self.send(req) {
            Err(AppError::Validation(msg)) if msg == TOO_LARGE && changes.len() > 1 => {
                let (a, b) = changes.split_at(changes.len() / 2);
                Ok(self.push_batch(device_id, a)? + self.push_batch(device_id, b)?)
            }
            Err(e) => Err(e),
            Ok(body) => {
                let accepted: Accepted = parse(body)?;
                Ok(accepted.accepted)
            }
        }
    }
}

const TOO_LARGE: &str = "sync batch too large";

/// Map an HTTP status onto the protocol's failure handling.
fn check_status(status: StatusCode) -> AppResult<()> {
    if status.is_success() {
        return Ok(());
    }
    Err(match status {
        StatusCode::UNAUTHORIZED => AppError::Config(TOKEN_REJECTED.into()),
        StatusCode::PAYLOAD_TOO_LARGE => AppError::Validation(TOO_LARGE.into()),
        s if s.is_server_error() => AppError::Network(format!("sync server error {}", s.as_u16())),
        s => AppError::Validation(format!("sync server answered {}", s.as_u16())),
    })
}

fn parse<T: serde::de::DeserializeOwned>(body: serde_json::Value) -> AppResult<T> {
    // serde's message can quote the offending value, i.e. server-supplied
    // text; keep it out of `last_error` (shown on the card and to agents).
    serde_json::from_value(body).map_err(|e| {
        log::warn!("[sync] malformed server response ({:?})", e.classify());
        AppError::Parse("malformed sync server response".into())
    })
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Health {
    protocol_version: u32,
}

#[derive(Deserialize)]
struct Accepted {
    accepted: usize,
}

impl Transport for HttpTransport {
    fn health(&self) -> AppResult<u32> {
        let req = crate::net::http::shared().get(self.url("/health"));
        Ok(parse::<Health>(self.send(req)?)?.protocol_version)
    }

    fn pull(&self, since: Option<&str>) -> AppResult<Page> {
        let mut req = crate::net::http::shared().get(self.url("/changes"));
        if let Some(cursor) = since {
            req = req.query(&[("since", cursor)]);
        }
        parse(self.send(req)?)
    }

    fn push(&self, device_id: &str, changes: &[Envelope]) -> AppResult<usize> {
        let mut stored = 0;
        for batch in changes.chunks(PUSH_BATCH) {
            stored += self.push_batch(device_id, batch)?;
        }
        Ok(stored)
    }
}
