//! `HttpTransport` against a real local HTTP server: the wire format, the
//! protocol's failure handling, and two devices with every real store syncing
//! through a stateful v1 server over HTTP.

use std::sync::Mutex as StdMutex;

use wiremock::matchers::{any, header, method, path, query_param};
use wiremock::{Mock, MockServer, Request, Respond, ResponseTemplate};

use super::stores::RealDevice;
use super::*;
use crate::sync::http::{HttpTransport, TOKEN_REJECTED};

/// Run `f` with a transport to `server` on a blocking thread, as the
/// scheduler does.
async fn blocking<R: Send + 'static>(
    server: &MockServer,
    token: &str,
    f: impl FnOnce(&HttpTransport) -> R + Send + 'static,
) -> R {
    let transport = HttpTransport::new(&server.uri(), token, tokio::runtime::Handle::current());
    tokio::task::spawn_blocking(move || f(&transport))
        .await
        .unwrap()
}

fn envelope(id: &str) -> Envelope {
    Envelope {
        store: "items".into(),
        record_id: id.into(),
        rev: 1,
        device_id: "d".into(),
        deleted: false,
        schema_version: 1,
        payload: json!({ "v": id }),
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn health_sends_the_bearer_token_and_reads_the_protocol_version() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/health"))
        .and(header("authorization", "Bearer secret"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "protocolVersion": 1 })))
        .expect(1)
        .mount(&server)
        .await;
    let v = blocking(&server, "secret", |t| t.health()).await.unwrap();
    assert_eq!(v, 1);
}

#[tokio::test(flavor = "multi_thread")]
async fn pull_passes_the_cursor_and_parses_the_page() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/changes"))
        .and(query_param("since", "c7"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "changes": [envelope("1")], "cursor": "c8", "hasMore": false
        })))
        .mount(&server)
        .await;
    let page = blocking(&server, "t", |t| t.pull(Some("c7")))
        .await
        .unwrap();
    assert_eq!(page.cursor, "c8");
    assert_eq!(page.changes, vec![envelope("1")]);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_401_is_a_rejected_token_and_never_echoes_it() {
    let server = MockServer::start().await;
    Mock::given(path("/health"))
        .respond_with(ResponseTemplate::new(401))
        .mount(&server)
        .await;
    let err = blocking(&server, "super-secret-token", |t| t.health())
        .await
        .unwrap_err();
    assert!(matches!(&err, AppError::Config(m) if m == TOKEN_REJECTED));
    assert!(!err.to_string().contains("super-secret-token"));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_5xx_is_a_network_error_and_a_bad_body_is_a_parse_error() {
    let server = MockServer::start().await;
    Mock::given(path("/health"))
        .respond_with(ResponseTemplate::new(503))
        .mount(&server)
        .await;
    Mock::given(path("/changes"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "nope": true })))
        .mount(&server)
        .await;
    let err = blocking(&server, "t", |t| t.health()).await.unwrap_err();
    assert!(matches!(err, AppError::Network(_)), "{err:?}");
    let err = blocking(&server, "t", |t| t.pull(None)).await.unwrap_err();
    assert!(matches!(err, AppError::Parse(_)), "{err:?}");
}

/// Answers `413` to any batch over two envelopes, else accepts it.
struct SmallBatchesOnly;

impl Respond for SmallBatchesOnly {
    fn respond(&self, req: &Request) -> ResponseTemplate {
        let body: Value = serde_json::from_slice(&req.body).unwrap();
        let n = body["changes"].as_array().unwrap().len();
        if n > 2 {
            ResponseTemplate::new(413)
        } else {
            ResponseTemplate::new(200).set_body_json(json!({ "accepted": n }))
        }
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn a_413_splits_the_batch_until_the_server_takes_it() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/changes"))
        .respond_with(SmallBatchesOnly)
        .mount(&server)
        .await;
    let changes: Vec<Envelope> = (0..7).map(|i| envelope(&i.to_string())).collect();
    let stored = blocking(&server, "t", move |t| t.push("dev", &changes))
        .await
        .unwrap();
    assert_eq!(stored, 7);
}

/// A minimal, stateful v1 server: one namespace, the cursor is the log length.
#[derive(Default)]
struct V1Server {
    log: StdMutex<Vec<Value>>,
}

impl Respond for V1Server {
    fn respond(&self, req: &Request) -> ResponseTemplate {
        let auth = req
            .headers
            .get("authorization")
            .and_then(|v| v.to_str().ok());
        if auth != Some("Bearer shared-token") {
            return ResponseTemplate::new(401);
        }
        let mut log = self.log.lock().unwrap();
        match (req.method.as_str(), req.url.path()) {
            ("GET", "/health") => {
                ResponseTemplate::new(200).set_body_json(json!({ "protocolVersion": 1 }))
            }
            ("POST", "/changes") => {
                let body: Value = serde_json::from_slice(&req.body).unwrap();
                let changes = body["changes"].as_array().unwrap().clone();
                let n = changes.len();
                log.extend(changes);
                ResponseTemplate::new(200).set_body_json(json!({ "accepted": n }))
            }
            ("GET", "/changes") => {
                let from: usize = req
                    .url
                    .query_pairs()
                    .find(|(k, _)| k == "since")
                    .and_then(|(_, v)| v.parse().ok())
                    .unwrap_or(0);
                let to = (from + 50).min(log.len());
                ResponseTemplate::new(200).set_body_json(json!({
                    "changes": log[from..to],
                    "cursor": to.to_string(),
                    "hasMore": to < log.len(),
                }))
            }
            _ => ResponseTemplate::new(404),
        }
    }
}

async fn v1_server() -> MockServer {
    let server = MockServer::start().await;
    Mock::given(any())
        .respond_with(V1Server::default())
        .mount(&server)
        .await;
    server
}

#[tokio::test(flavor = "multi_thread")]
async fn two_devices_with_every_store_converge_over_http() {
    let server = v1_server().await;
    let uri = server.uri();
    let rt = tokio::runtime::Handle::current();
    tokio::task::spawn_blocking(move || {
        let transport = HttpTransport::new(&uri, "shared-token", rt);
        let (a, b) = (RealDevice::new(), RealDevice::new());
        a.seed();
        a.sync_via(&transport, 10).unwrap();
        b.sync_via(&transport, 20).unwrap();
        assert_eq!(a.records(), b.records());
        assert_eq!(b.count("applications"), 1);

        b.referrals.delete_record("ref-1").unwrap();
        b.sync_via(&transport, 30).unwrap();
        a.sync_via(&transport, 40).unwrap();
        assert_eq!(a.count("referrals"), 0);
        assert_eq!(a.records(), b.records());
    })
    .await
    .unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn a_device_with_the_wrong_token_gets_nothing() {
    let server = v1_server().await;
    let uri = server.uri();
    let rt = tokio::runtime::Handle::current();
    tokio::task::spawn_blocking(move || {
        let a = RealDevice::new();
        a.seed();
        let wrong = HttpTransport::new(&uri, "someone-else", rt);
        let err = a.sync_via(&wrong, 10).unwrap_err();
        assert!(matches!(&err, AppError::Config(m) if m == TOKEN_REJECTED));
    })
    .await
    .unwrap();
}
