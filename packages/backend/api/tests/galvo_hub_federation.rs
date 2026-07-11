//! End-to-end Galvo Hub mTLS A2A federation round trip, against the REAL Hub
//! running in-process.
//!
//! Flow exercised entirely through Celerex's own `connectors::galvo_hub`:
//!   1. boot galvo-hub (mTLS RPC + a debug plaintext /push port)
//!   2. Celerex registers two agents (celerex/caller, acme/responder) — real
//!      CSR → issued client cert, real A2A keys
//!   3. elevate the caller org to `verified` so it clears the responder's
//!      default trust policy
//!   4. responder runs the /push challenge-response receiver
//!   5. caller invokes the responder (lookup → cap token → seal → relay)
//!   6. responder decrypts the relayed envelope and we assert the plaintext +
//!      sender handle survived the round trip

use celerex_api::connectors::galvo_hub as cx_hub; // Celerex's client
use cx_hub::{GalvoHubClient, RegisterRequest};
use galvo_hub::config::HubConfig; // the real Hub crate
use std::time::Duration;
use tempfile::TempDir;

fn pick_port() -> u16 {
    std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

async fn register_agent(
    hub_url: &str,
    ca_pem: &str,
    token: &str,
    org: &str,
    slug: &str,
) -> cx_hub::HubKeystore {
    cx_hub::register(RegisterRequest {
        hub_url: hub_url.to_string(),
        bootstrap_token: token.to_string(),
        ca_cert_pem: ca_pem.to_string(),
        org_id: org.to_string(),
        org_name: org.to_string(),
        agent_slug: slug.to_string(),
        agent_description: format!("Celerex {slug}"),
        capabilities: vec!["celerex.flow".into()],
    })
    .await
    .expect("register")
}

#[tokio::test]
async fn celerex_to_celerex_a2a_round_trip_through_hub() {
    // 1. Boot the real Hub in-process: mTLS RPC + plaintext /push (debug only).
    let dir = TempDir::new().unwrap();
    let rpc_port = pick_port();
    let ws_port = pick_port();
    let token = "celerex-boot-token".to_string();
    let cfg = HubConfig {
        addr: format!("127.0.0.1:{rpc_port}"),
        data_dir: dir.path().into(),
        bootstrap_token: token.clone(),
        issuer_seed_hex: None,
        require_executor_signatures: false,
        plaintext_ws_port: Some(ws_port),
        plaintext_api_port: None,
    };
    tokio::spawn(async move {
        let _ = galvo_hub::server::run(cfg).await;
    });
    tokio::time::sleep(Duration::from_millis(500)).await;

    let hub_url = format!("https://127.0.0.1:{rpc_port}");
    let ca_pem = std::fs::read_to_string(dir.path().join("ca.crt")).unwrap();

    // 2. Register both Celerex agents (different orgs → trust policy applies).
    let caller_ks = register_agent(&hub_url, &ca_pem, &token, "celerex", "caller").await;
    let responder_ks = register_agent(&hub_url, &ca_pem, &token, "acme", "responder").await;
    assert!(caller_ks.issued_cert_pem.contains("BEGIN CERTIFICATE"));
    assert_eq!(responder_ks.handle, "acme/responder");

    // 3. Elevate the caller org so it clears the responder's default Verified policy.
    cx_hub::set_org_tier(&hub_url, &ca_pem, &token, "celerex", "verified")
        .await
        .expect("set_org_tier");

    // 4. Responder receiver: forward decrypted plaintext + sender out via a channel.
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<(String, Vec<u8>)>();
    let ws_url = format!("ws://127.0.0.1:{ws_port}/push");
    let responder_client = GalvoHubClient::new(responder_ks);
    tokio::spawn(async move {
        let _ = responder_client
            .run_receiver(&ws_url, move |header, plaintext| {
                let _ = tx.send((header.from, plaintext));
            })
            .await;
    });
    // Let the /push challenge-response handshake register the responder's tx.
    tokio::time::sleep(Duration::from_millis(700)).await;

    // 5. Caller invokes the responder over the encrypted, Hub-relayed path.
    let payload = br#"{"role":"user","parts":[{"type":"text","text":"federate this"}]}"#;
    let caller_client = GalvoHubClient::new(caller_ks);
    let env = caller_client
        .invoke("acme/responder", payload)
        .await
        .expect("invoke relayed");
    assert_eq!(env.header.from, "celerex/caller");
    assert_eq!(env.header.to, "acme/responder");
    // The header is cleartext; the body must NOT be.
    assert!(!env.ciphertext_b64.is_empty());

    // 6. Responder received + decrypted it.
    let (from, plaintext) = tokio::time::timeout(Duration::from_secs(5), rx.recv())
        .await
        .expect("receiver delivered within 5s")
        .expect("channel open");
    assert_eq!(from, "celerex/caller");
    assert_eq!(plaintext, payload, "decrypted plaintext must match what the caller sealed");
}

#[tokio::test]
async fn invoke_without_trust_elevation_is_rejected() {
    // Same setup, but DON'T elevate the caller — the cap-token request must be
    // refused by the target's default Verified policy, so invoke errors.
    let dir = TempDir::new().unwrap();
    let rpc_port = pick_port();
    let token = "celerex-boot-token".to_string();
    let cfg = HubConfig {
        addr: format!("127.0.0.1:{rpc_port}"),
        data_dir: dir.path().into(),
        bootstrap_token: token.clone(),
        issuer_seed_hex: None,
        require_executor_signatures: false,
        plaintext_ws_port: None,
        plaintext_api_port: None,
    };
    tokio::spawn(async move {
        let _ = galvo_hub::server::run(cfg).await;
    });
    tokio::time::sleep(Duration::from_millis(500)).await;

    let hub_url = format!("https://127.0.0.1:{rpc_port}");
    let ca_pem = std::fs::read_to_string(dir.path().join("ca.crt")).unwrap();
    let caller_ks = register_agent(&hub_url, &ca_pem, &token, "mint-org", "caller").await;
    let _responder_ks = register_agent(&hub_url, &ca_pem, &token, "acme", "responder").await;

    let caller = GalvoHubClient::new(caller_ks);
    let err = caller.invoke("acme/responder", b"should be blocked").await.unwrap_err();
    // The cap-token request is trust-policy-rejected; invoke surfaces it.
    assert!(
        format!("{err}").contains("trust_policy_rejected") || format!("{err}").contains("cap_token"),
        "expected trust rejection, got: {err}"
    );
}
