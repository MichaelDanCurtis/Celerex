mod common;
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use rsa::pkcs1::EncodeRsaPrivateKey;
use rsa::traits::PublicKeyParts;
use rsa::{RsaPrivateKey, RsaPublicKey};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn b64u(bytes: &[u8]) -> String {
    URL_SAFE_NO_PAD.encode(bytes)
}

/// (signing PEM, JWKS json) for a fresh RSA key with the given kid.
fn keypair(kid: &str) -> (String, serde_json::Value) {
    let mut rng = rand::thread_rng();
    let priv_key = RsaPrivateKey::new(&mut rng, 2048).expect("rsa key");
    let pem = priv_key
        .to_pkcs1_pem(rsa::pkcs1::LineEnding::LF)
        .unwrap()
        .to_string();
    let pub_key = RsaPublicKey::from(&priv_key);
    let n = b64u(&pub_key.n().to_bytes_be());
    let e = b64u(&pub_key.e().to_bytes_be());
    let jwks = serde_json::json!({ "keys": [
        { "kty": "RSA", "use": "sig", "alg": "RS256", "kid": kid, "n": n, "e": e }
    ]});
    (pem, jwks)
}

fn sign(pem: &str, kid: &str, sub: &str) -> String {
    let mut header = jsonwebtoken::Header::new(jsonwebtoken::Algorithm::RS256);
    header.kid = Some(kid.to_string());
    let claims = serde_json::json!({ "sub": sub, "exp": (chrono::Utc::now().timestamp() + 3600) as usize });
    let key = jsonwebtoken::EncodingKey::from_rsa_pem(pem.as_bytes()).unwrap();
    jsonwebtoken::encode(&header, &claims, &key).unwrap()
}

async fn setup_jwks(jwks: serde_json::Value) -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/sso/jwks/client_test"))
        .respond_with(ResponseTemplate::new(200).set_body_json(jwks))
        .mount(&server)
        .await;
    server
}

// NOTE on test isolation: both scenarios set the same env vars
// (WORKOS_CLIENT_ID, WORKOS_JWKS_URL) pointing at per-scenario mock servers.
// cargo runs integration tests in the same binary, possibly in parallel
// threads, so setting those vars from two `#[tokio::test]`s would race
// (see the rust-integration-test-env-var-cross-binary-race trap). The
// simplest reliable fix is to keep this to a SINGLE test that exercises
// both the happy path (provision + mint + idempotency + protected-route
// acceptance) and the invalid-token path within one env scope.
#[tokio::test]
async fn workos_login_provisions_mints_and_rejects_invalid() {
    let (pem, jwks) = keypair("kid-1");
    let (other_pem, _other) = keypair("kid-1"); // different key, same kid → bad signature
    let jwks_server = setup_jwks(jwks).await;
    // SAFETY: single-process test; set before build_app reads env.
    unsafe {
        std::env::set_var("WORKOS_CLIENT_ID", "client_test");
        std::env::set_var("WORKOS_JWKS_URL", jwks_server.uri());
    }
    let db = common::start_test_db().await;
    let server = axum_test::TestServer::new(celerex_api::build_app(db.pool.clone())).unwrap();

    // --- Happy path: provision + mint session ---
    let token = sign(&pem, "kid-1", "wos_user_123");
    let r1 = server
        .post("/api/auth/workos")
        .add_header("authorization", format!("Bearer {token}"))
        .json(&serde_json::json!({ "email": "alice@acme.test" }))
        .await;
    r1.assert_status_ok();
    let body1: serde_json::Value = r1.json();
    let eng1 = body1["engagement"]["id"].as_str().unwrap().to_string();
    let session = body1["token"].as_str().unwrap().to_string();

    // The minted session is accepted by an existing protected route.
    let projects = server
        .get("/api/projects")
        .add_header("authorization", format!("Bearer {session}"))
        .await;
    projects.assert_status_ok();

    // --- Idempotency: second login (same WorkOS sub) → same engagement ---
    let token2 = sign(&pem, "kid-1", "wos_user_123");
    let r2 = server
        .post("/api/auth/workos")
        .add_header("authorization", format!("Bearer {token2}"))
        .json(&serde_json::json!({ "email": "alice@acme.test" }))
        .await;
    r2.assert_status_ok();
    assert_eq!(
        r2.json::<serde_json::Value>()["engagement"]["id"]
            .as_str()
            .unwrap(),
        eng1
    );

    // --- Invalid token: signed by a different key (sig mismatch) → 401 ---
    let bad = sign(&other_pem, "kid-1", "wos_user_x");
    let resp = server
        .post("/api/auth/workos")
        .add_header("authorization", format!("Bearer {bad}"))
        .json(&serde_json::json!({ "email": "x@y.test" }))
        .await;
    resp.assert_status(axum::http::StatusCode::UNAUTHORIZED);
}
