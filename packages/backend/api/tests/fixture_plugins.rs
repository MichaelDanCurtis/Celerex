use celerex_api::plugins::fixtures::{stargate::StargateFixture, workiq::WorkIqFixture};

#[tokio::test]
async fn workiq_fixture_returns_canned_payload() {
    let f = WorkIqFixture::from_fixture_dir();
    let out = f
        .invoke(
            "workiq.pull_metrics",
            serde_json::json!({"window_days": 7}),
        )
        .await
        .unwrap();
    assert_eq!(out["metrics"]["emails_sent"], 1284);
}

#[tokio::test]
async fn stargate_fixture_returns_canned_rows() {
    let f = StargateFixture::from_fixture_dir();
    let out = f
        .invoke("stargate.query", serde_json::json!({"sql":"SELECT *"}))
        .await
        .unwrap();
    assert_eq!(out["rows"][0][0], "Acme Co");
}
