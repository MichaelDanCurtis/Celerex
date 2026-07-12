//! Gated LIVE end-to-end checks against the REAL provider endpoints. These make
//! real network calls with real credentials and are `#[ignore]`d, so the default
//! `cargo test` (and CI) never runs them. Run manually, one provider at a time:
//!
//!   XAI_LIVE_BLOB='{"access_token":"...","refresh_token":"...","expires_at":0}' \
//!     cargo test -p celerex-api --test xai_live_e2e xai_live_completes -- --ignored --nocapture
//!   KIMI_CODE_API_KEY=sk-... \
//!     cargo test -p celerex-api --test xai_live_e2e kimi_code_live_completes -- --ignored --nocapture
//!   GLM_API_KEY=... \
//!     cargo test -p celerex-api --test xai_live_e2e glm_live_completes -- --ignored --nocapture
//!
//! Never commit real credentials. Model ids below may drift; adjust as needed.

use celerex_orchestrator::llm::provider::Provider;
use serde_json::json;

#[tokio::test]
#[ignore = "live: requires a real vaulted xAI token blob in XAI_LIVE_BLOB"]
async fn xai_live_completes() {
    let Ok(blob) = std::env::var("XAI_LIVE_BLOB") else {
        eprintln!("skip: XAI_LIVE_BLOB not set — this live test needs real creds");
        return;
    };
    let p = celerex_api::llm::xai::XaiProvider::from_blob(&blob).expect("blob parses");
    let out = p
        .complete(
            "grok-4.3",
            json!([{"role":"user","content":"reply with the single word pong"}]),
            json!([]),
        )
        .await
        .expect("live xai completion");
    assert!(out.raw["choices"][0]["message"]["content"].is_string());
}

#[tokio::test]
#[ignore = "live: requires KIMI_CODE_API_KEY (Kimi for Coding plan)"]
async fn kimi_code_live_completes() {
    let Ok(key) = std::env::var("KIMI_CODE_API_KEY") else {
        eprintln!("skip: KIMI_CODE_API_KEY not set — this live test needs real creds");
        return;
    };
    let p = celerex_api::llm::openai_compat::OpenAiCompatProvider::new(
        "https://api.kimi.com/coding/v1".into(),
        key,
    );
    let out = p
        .complete(
            "kimi-k2.5",
            json!([{"role":"user","content":"say pong"}]),
            json!([]),
        )
        .await
        .expect("live kimi-code completion");
    assert!(out.raw["choices"][0]["message"]["content"].is_string());
}

#[tokio::test]
#[ignore = "live: requires GLM_API_KEY (GLM Coding Plan key)"]
async fn glm_live_completes() {
    let Ok(key) = std::env::var("GLM_API_KEY") else {
        eprintln!("skip: GLM_API_KEY not set — this live test needs real creds");
        return;
    };
    let p = celerex_api::llm::openai_compat::OpenAiCompatProvider::new(
        "https://api.z.ai/api/coding/paas/v4".into(),
        key,
    );
    let out = p
        .complete(
            "glm-5.2",
            json!([{"role":"user","content":"say pong"}]),
            json!([]),
        )
        .await
        .expect("live glm completion");
    assert!(out.raw["choices"][0]["message"]["content"].is_string());
}
