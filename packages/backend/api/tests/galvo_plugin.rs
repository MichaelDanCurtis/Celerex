mod common;

use celerex_orchestrator::execution_type::ExecutionType;
use celerex_orchestrator::executor::{Executor, InMemoryRunStore, RunStore};
use celerex_orchestrator::handlers::agent::AgentHandler;
use celerex_orchestrator::handlers::agent_stub::FixtureA2AClient;
use celerex_orchestrator::job::{JobRequest, RunStatus, Step, StepKind};
use celerex_orchestrator::tool_dispatch::Dispatcher;
use jsonwebtoken::{encode, EncodingKey, Header};
use serde::Serialize;
use serde_json::{json, Value};
use std::sync::Arc;
use uuid::Uuid;

const GALVO_MANIFEST: &str = include_str!("fixtures/plugins/galvo/manifest.json");
const GALVO_FIXTURE_MD: &str = include_str!("fixtures/plugins/galvo/research_weekly.md");

#[derive(Serialize)]
struct Claims { sub: String, engagement_id: String, project_id: Option<String>, exp: usize }

fn token(eng: Uuid) -> String {
    // SAFETY: tests serialized within one binary; env set before middleware reads it.
    unsafe { std::env::set_var("CELEREX_JWT_SECRET", "test-secret"); }
    let c = Claims {
        sub: "u".into(),
        engagement_id: eng.to_string(),
        project_id: None,
        exp: (chrono::Utc::now().timestamp() + 3600) as usize,
    };
    encode(&Header::default(), &c, &EncodingKey::from_secret(b"test-secret")).unwrap()
}

#[tokio::test]
async fn galvo_plugin_manifest_installs_and_lists_outbound_capability() {
    let db = common::start_test_db().await;
    let app = celerex_api::build_app(db.pool.clone());
    let server = axum_test::TestServer::new(app).unwrap();

    let eng: Uuid = sqlx::query_scalar("INSERT INTO engagements (name) VALUES ('E') RETURNING id")
        .fetch_one(&db.pool).await.unwrap();
    let t = token(eng);

    let manifest: Value = serde_json::from_str(GALVO_MANIFEST).unwrap();
    let install_resp = server.post("/api/plugins")
        .add_header("authorization", format!("Bearer {t}"))
        .json(&json!({ "manifest": manifest })).await;
    install_resp.assert_status(axum::http::StatusCode::CREATED);

    let list = server.get("/api/plugins")
        .add_header("authorization", format!("Bearer {t}")).await;
    let arr: Vec<Value> = list.json();
    assert_eq!(arr.len(), 1);
    assert_eq!(arr[0]["plugin_id"], "galvo");
    assert_eq!(arr[0]["manifest"]["outbound"][0]["id"], "galvo.research.weekly");
    assert_eq!(arr[0]["manifest"]["outbound"][0]["kind"], "agent");
}

#[tokio::test]
async fn galvo_capability_invokable_through_orchestrator_agent_handler() {
    let client = Arc::new(
        FixtureA2AClient::new().with_fixture("galvo.research.weekly", GALVO_FIXTURE_MD)
    );
    let mut dispatcher = Dispatcher::new();
    dispatcher.register(ExecutionType::Agent, Arc::new(AgentHandler { client }));

    let run_store: Arc<dyn RunStore> = Arc::new(InMemoryRunStore::new());
    let executor = Executor::new(Arc::new(dispatcher), run_store.clone());
    let run_id = Uuid::new_v4();
    let job = JobRequest {
        run_id,
        engagement_id: Uuid::new_v4(),
        project_id: None,
        steps: vec![Step {
            id: "s1".into(),
            depends_on: vec![],
            kind: StepKind::Tool {
                execution: ExecutionType::Agent,
                config: json!({ "target": "galvo.research.weekly" }),
                inputs: json!({}),
            },
        }],
    };

    let status = executor.run(job).await.unwrap();
    assert_eq!(status, RunStatus::Succeeded);

    let records = run_store.records_for(run_id);
    assert_eq!(records.len(), 1);
    let outputs = records[0].outputs.as_ref().expect("outputs");
    let result = outputs["result"].as_str().expect("result string");
    assert!(result.contains("# Weekly Research Digest"),
            "fixture payload must reach the step output: {outputs}");
}

#[tokio::test]
async fn unknown_galvo_target_fails_through_handler() {
    let client = Arc::new(FixtureA2AClient::new());
    let mut dispatcher = Dispatcher::new();
    dispatcher.register(ExecutionType::Agent, Arc::new(AgentHandler { client }));

    let run_store: Arc<dyn RunStore> = Arc::new(InMemoryRunStore::new());
    let executor = Executor::new(Arc::new(dispatcher), run_store);
    let job = JobRequest {
        run_id: Uuid::new_v4(),
        engagement_id: Uuid::new_v4(),
        project_id: None,
        steps: vec![Step {
            id: "s1".into(),
            depends_on: vec![],
            kind: StepKind::Tool {
                execution: ExecutionType::Agent,
                config: json!({ "target": "galvo.unknown.thing" }),
                inputs: json!({}),
            },
        }],
    };
    let status = executor.run(job).await.unwrap();
    assert_eq!(status, RunStatus::Failed);
}
