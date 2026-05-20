mod common;

use async_trait::async_trait;
use celerex_api::plugins::fixtures::galvatronic::GalvatronicFixture;
use celerex_orchestrator::execution_type::ExecutionType;
use celerex_orchestrator::executor::{Executor, InMemoryRunStore, RunStore};
use celerex_orchestrator::job::{JobRequest, RunStatus, Step, StepKind};
use celerex_orchestrator::tool_dispatch::{DispatchError, Dispatcher, Handler};
use jsonwebtoken::{encode, EncodingKey, Header};
use serde::Serialize;
use serde_json::{json, Value};
use std::sync::Arc;
use uuid::Uuid;

const GALVATRONIC_MANIFEST: &str =
    include_str!("fixtures/plugins/galvatronic/manifest.json");

#[derive(Serialize)]
struct Claims {
    sub: String,
    engagement_id: String,
    project_id: Option<String>,
    exp: usize,
}

fn token(eng: Uuid) -> String {
    // SAFETY: tests serialized within one binary; env set before middleware reads it.
    unsafe {
        std::env::set_var("CELEREX_JWT_SECRET", "test-secret");
    }
    let c = Claims {
        sub: "u".into(),
        engagement_id: eng.to_string(),
        project_id: None,
        exp: (chrono::Utc::now().timestamp() + 3600) as usize,
    };
    encode(
        &Header::default(),
        &c,
        &EncodingKey::from_secret(b"test-secret"),
    )
    .unwrap()
}

/// Handler that proxies an `ExecutionType::Http` step to the
/// Galvatronic fixture. The capability id comes from
/// `config.capability`, mirroring the WorkIQ wiring used in
/// `e2e_jean_newsletter`.
struct GalvatronicHandler {
    fixture: GalvatronicFixture,
}

#[async_trait]
impl Handler for GalvatronicHandler {
    async fn invoke(&self, config: Value, inputs: Value) -> Result<Value, DispatchError> {
        let capability = config
            .get("capability")
            .and_then(|v| v.as_str())
            .ok_or_else(|| DispatchError::Handler("missing capability".into()))?;
        self.fixture
            .invoke(capability, inputs)
            .await
            .map_err(DispatchError::Handler)
    }
}

#[tokio::test]
async fn galvatronic_manifest_installs_and_lists_three_outbound_capabilities() {
    let db = common::start_test_db().await;
    let app = celerex_api::build_app(db.pool.clone());
    let server = axum_test::TestServer::new(app).unwrap();

    let eng: Uuid =
        sqlx::query_scalar("INSERT INTO engagements (name) VALUES ('E') RETURNING id")
            .fetch_one(&db.pool)
            .await
            .unwrap();
    let t = token(eng);

    let manifest: Value = serde_json::from_str(GALVATRONIC_MANIFEST).unwrap();
    let install_resp = server
        .post("/api/plugins")
        .add_header("authorization", format!("Bearer {t}"))
        .json(&json!({ "manifest": manifest }))
        .await;
    install_resp.assert_status(axum::http::StatusCode::CREATED);

    let list = server
        .get("/api/plugins")
        .add_header("authorization", format!("Bearer {t}"))
        .await;
    let arr: Vec<Value> = list.json();
    assert_eq!(arr.len(), 1);
    assert_eq!(arr[0]["plugin_id"], "galvatronic");
    let outbound = arr[0]["manifest"]["outbound"].as_array().expect("outbound");
    assert_eq!(outbound.len(), 3);
    let ids: Vec<&str> = outbound
        .iter()
        .map(|c| c["id"].as_str().unwrap())
        .collect();
    assert!(ids.contains(&"galvatronic.read_sensor"));
    assert!(ids.contains(&"galvatronic.list_devices"));
    assert!(ids.contains(&"galvatronic.send_command"));
    for cap in outbound {
        assert_eq!(cap["kind"], "http");
    }
}

#[tokio::test]
async fn galvatronic_read_sensor_returns_canned_reading() {
    let fixture = GalvatronicFixture::from_fixture_dir();
    let out = fixture
        .invoke(
            "galvatronic.read_sensor",
            json!({ "sensor_id": "press-line-3-temp" }),
        )
        .await
        .unwrap();
    assert_eq!(out["sensor_id"], "press-line-3-temp");
    assert_eq!(out["unit"], "celsius");
    assert!(out["value"].is_number(), "value present: {out}");
    assert!(out["timestamp"].is_string(), "timestamp present: {out}");
}

#[tokio::test]
async fn galvatronic_list_devices_returns_canned_inventory() {
    let fixture = GalvatronicFixture::from_fixture_dir();
    let out = fixture
        .invoke("galvatronic.list_devices", json!({}))
        .await
        .unwrap();
    let devices = out["devices"].as_array().expect("devices");
    assert_eq!(devices.len(), 3);
    assert_eq!(devices[0]["device_id"], "press-line-3");
}

#[tokio::test]
async fn galvatronic_send_command_synthesises_acceptance() {
    let fixture = GalvatronicFixture::from_fixture_dir();
    let out = fixture
        .invoke(
            "galvatronic.send_command",
            json!({ "device_id": "press-line-3", "command": "shutdown" }),
        )
        .await
        .unwrap();
    assert_eq!(out["accepted"], true);
    assert_eq!(out["device_id"], "press-line-3");
    assert_eq!(out["command"], "shutdown");
    assert!(out["command_id"].as_str().unwrap().contains("press-line-3"));
}

#[tokio::test]
async fn galvatronic_unknown_capability_fails() {
    let fixture = GalvatronicFixture::from_fixture_dir();
    let err = fixture
        .invoke("galvatronic.bogus", json!({}))
        .await
        .unwrap_err();
    assert!(err.contains("unknown galvatronic capability"));
}

#[tokio::test]
async fn galvatronic_capability_invokable_through_orchestrator_http_handler() {
    let fixture = GalvatronicFixture::from_fixture_dir();
    let mut dispatcher = Dispatcher::new();
    dispatcher.register(
        ExecutionType::Http,
        Arc::new(GalvatronicHandler { fixture }),
    );

    let run_store: Arc<dyn RunStore> = Arc::new(InMemoryRunStore::new());
    let executor = Executor::new(Arc::new(dispatcher), run_store.clone());
    let run_id = Uuid::new_v4();

    // A small pipeline:
    //   read_sensor  →  send_command(device_id from read_sensor.sensor_id)
    // exercising both the canned-file path and the input-driven synth path,
    // plus the templating bridge between steps.
    let job = JobRequest {
        run_id,
        engagement_id: Uuid::new_v4(),
        project_id: None,
        steps: vec![
            Step {
                id: "read".into(),
                depends_on: vec![],
                kind: StepKind::Tool {
                    execution: ExecutionType::Http,
                    config: json!({ "capability": "galvatronic.read_sensor" }),
                    inputs: json!({ "sensor_id": "press-line-3-temp" }),
                },
            },
            Step {
                id: "react".into(),
                depends_on: vec!["read".into()],
                kind: StepKind::Tool {
                    execution: ExecutionType::Http,
                    config: json!({ "capability": "galvatronic.send_command" }),
                    inputs: json!({
                        "device_id": "{{steps.read.outputs.sensor_id}}",
                        "command":   "alert"
                    }),
                },
            },
        ],
    };

    let status = executor.run(job).await.unwrap();
    assert_eq!(status, RunStatus::Succeeded);

    let records = run_store.records_for(run_id);
    assert_eq!(records.len(), 2);
    let read = records.iter().find(|r| r.step_id == "read").unwrap();
    let read_out = read.outputs.as_ref().expect("read outputs");
    assert_eq!(read_out["sensor_id"], "press-line-3-temp");
    assert_eq!(read_out["unit"], "celsius");

    let react = records.iter().find(|r| r.step_id == "react").unwrap();
    let react_out = react.outputs.as_ref().expect("react outputs");
    assert_eq!(react_out["accepted"], true);
    assert_eq!(react_out["device_id"], "press-line-3-temp");
    assert_eq!(react_out["command"], "alert");
}

#[tokio::test]
async fn galvatronic_unknown_capability_fails_through_handler() {
    let mut dispatcher = Dispatcher::new();
    dispatcher.register(
        ExecutionType::Http,
        Arc::new(GalvatronicHandler {
            fixture: GalvatronicFixture::from_fixture_dir(),
        }),
    );

    let run_store: Arc<dyn RunStore> = Arc::new(InMemoryRunStore::new());
    let executor = Executor::new(Arc::new(dispatcher), run_store);
    let job = JobRequest {
        run_id: Uuid::new_v4(),
        engagement_id: Uuid::new_v4(),
        project_id: None,
        steps: vec![Step {
            id: "bad".into(),
            depends_on: vec![],
            kind: StepKind::Tool {
                execution: ExecutionType::Http,
                config: json!({ "capability": "galvatronic.bogus" }),
                inputs: json!({}),
            },
        }],
    };
    let status = executor.run(job).await.unwrap();
    assert_eq!(status, RunStatus::Failed);
}
