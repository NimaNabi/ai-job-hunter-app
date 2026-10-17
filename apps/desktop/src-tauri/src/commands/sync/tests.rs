//! `check_server`: nothing is saved unless the server speaks v1 AND accepts
//! the token on an authenticated request.

use serde_json::json;
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::check_server;
use crate::error::AppError;
use crate::sync::http::TOKEN_REJECTED;

async fn server(protocol: u32) -> MockServer {
    let server = MockServer::start().await;
    // An open /health, as the protocol allows.
    Mock::given(method("GET"))
        .and(path("/health"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(json!({ "protocolVersion": protocol })),
        )
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/changes"))
        .and(header("authorization", "Bearer good"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({ "changes": [], "cursor": "0", "hasMore": false })),
        )
        .mount(&server)
        .await;
    Mock::given(path("/changes"))
        .respond_with(ResponseTemplate::new(401))
        .mount(&server)
        .await;
    server
}

async fn check(server: &MockServer, token: &'static str) -> crate::error::AppResult<()> {
    let uri = server.uri();
    let rt = tokio::runtime::Handle::current();
    tokio::task::spawn_blocking(move || check_server(&uri, token, rt))
        .await
        .unwrap()
}

#[tokio::test(flavor = "multi_thread")]
async fn a_good_token_on_a_v1_server_passes() {
    let s = server(1).await;
    check(&s, "good").await.unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn a_wrong_token_fails_even_though_health_is_open() {
    let s = server(1).await;
    let err = check(&s, "wrong").await.unwrap_err();
    assert!(
        matches!(&err, AppError::Config(m) if m == TOKEN_REJECTED),
        "{err:?}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn another_protocol_version_fails() {
    let s = server(2).await;
    let err = check(&s, "good").await.unwrap_err();
    assert!(matches!(err, AppError::Validation(_)), "{err:?}");
}
