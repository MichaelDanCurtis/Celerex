// Integration tests for the automations foundation (migration 0017).
// Uses the same testcontainers + axum_test harness as design_rituals.rs.
mod common;

use async_trait::async_trait;
use serde_json::json;
use uuid::Uuid;

/// Seed: engagement → project → project_member. Returns (engagement_id, project_id, member_id).
async fn seed_member(pool: &sqlx::PgPool) -> (Uuid, Uuid, Uuid) {
    let eng: Uuid =
        sqlx::query_scalar("INSERT INTO engagements (name) VALUES ('AutomationTest') RETURNING id")
            .fetch_one(pool)
            .await
            .unwrap();
    let project: Uuid = sqlx::query_scalar(
        "INSERT INTO projects (engagement_id, name) VALUES ($1, 'AutoP') RETURNING id",
    )
    .bind(eng)
    .fetch_one(pool)
    .await
    .unwrap();
    let member: Uuid = sqlx::query_scalar(
        "INSERT INTO project_members (project_id, name, graph_json, provider_config_json)
         VALUES ($1, 'AutoM', $2, $3) RETURNING id",
    )
    .bind(project)
    .bind(json!({ "nodes": [], "edges": [] }))
    .bind(json!({}))
    .fetch_one(pool)
    .await
    .unwrap();
    (eng, project, member)
}

// ---------------------------------------------------------------------------
// (a) automation_triggers table has ≥8 columns
// ---------------------------------------------------------------------------
#[tokio::test]
async fn automation_triggers_table_has_at_least_8_columns() {
    let db = common::start_test_db().await;

    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM information_schema.columns
         WHERE table_name = 'automation_triggers'",
    )
    .fetch_one(&db.pool)
    .await
    .unwrap();

    assert!(
        count >= 8,
        "automation_triggers must have ≥8 columns, found {count}"
    );
}

// ---------------------------------------------------------------------------
// (b1) runs CHECK now accepts triggered_by='connector'
// ---------------------------------------------------------------------------
#[tokio::test]
async fn runs_accepts_triggered_by_connector() {
    let db = common::start_test_db().await;
    let (eng, project, member) = seed_member(&db.pool).await;

    let result = sqlx::query(
        "INSERT INTO runs (engagement_id, project_id, project_member_id, trigger, status, triggered_by)
         VALUES ($1, $2, $3, $4::jsonb, 'running', 'connector')",
    )
    .bind(eng)
    .bind(project)
    .bind(member)
    .bind(json!({"source": "connector"}).to_string())
    .execute(&db.pool)
    .await;

    assert!(
        result.is_ok(),
        "INSERT with triggered_by='connector' must succeed; got: {result:?}"
    );
}

// ---------------------------------------------------------------------------
// (b2) runs CHECK rejects triggered_by='bogus'
// ---------------------------------------------------------------------------
#[tokio::test]
async fn runs_rejects_triggered_by_bogus() {
    let db = common::start_test_db().await;
    let (eng, project, member) = seed_member(&db.pool).await;

    let result = sqlx::query(
        "INSERT INTO runs (engagement_id, project_id, project_member_id, trigger, status, triggered_by)
         VALUES ($1, $2, $3, $4::jsonb, 'running', 'bogus')",
    )
    .bind(eng)
    .bind(project)
    .bind(member)
    .bind(json!({"source": "bogus"}).to_string())
    .execute(&db.pool)
    .await;

    assert!(
        result.is_err(),
        "INSERT with triggered_by='bogus' must be rejected by the CHECK constraint"
    );
}

// ---------------------------------------------------------------------------
// (c) AutomationTriggersStore: due/mark_polled/cursor/list/update/delete
// ---------------------------------------------------------------------------
#[tokio::test]
async fn store_due_filters_by_interval_and_advances_cursor() {
    let db = common::start_test_db().await;
    let (eng, _project, member) = seed_member(&db.pool).await;

    use celerex_api::automations::store::{
        AutomationTrigger, AutomationTriggersStore, PgAutomationTriggersStore,
    };

    let store = PgAutomationTriggersStore::new(db.pool.clone());

    let id = store
        .create(AutomationTrigger {
            id: uuid::Uuid::new_v4(),
            project_member_id: member,
            connector_id: "http-api".into(),
            source_config: json!({"url":"http://x","items_path":"items","id_field":"id"}),
            filter: None,
            poll_interval_secs: 300,
            cursor: None,
            last_polled_at: None,
            enabled: true,
        })
        .await
        .unwrap();

    // never-polled trigger is due
    let due = store.due(chrono::Utc::now()).await.unwrap();
    assert!(
        due.iter().any(|t| t.id == id),
        "never-polled trigger should be due"
    );

    // after mark_polled it is not due within the interval, and the cursor persisted
    store
        .mark_polled(id, chrono::Utc::now(), Some("5".into()))
        .await
        .unwrap();
    let due2 = store.due(chrono::Utc::now()).await.unwrap();
    assert!(
        !due2.iter().any(|t| t.id == id),
        "just-polled trigger should not be due again immediately"
    );
    let got = store.get(id, eng).await.unwrap().unwrap();
    assert_eq!(got.cursor.as_deref(), Some("5"));

    // list_for_engagement returns it
    let listed = store.list_for_engagement(eng).await.unwrap();
    assert!(
        listed.iter().any(|t| t.id == id),
        "list_for_engagement should return the trigger"
    );

    // update -> enabled=false makes it not due even if interval would fire
    store
        .update(
            id,
            eng,
            &json!({"url":"http://y","items_path":"items","id_field":"id"}),
            None,
            300,
            false,
        )
        .await
        .unwrap();
    // reset last_polled_at to NULL so interval-check passes, but enabled=false should exclude it
    sqlx::query("UPDATE automation_triggers SET last_polled_at=NULL WHERE id=$1")
        .bind(id)
        .execute(&db.pool)
        .await
        .unwrap();
    let due3 = store.due(chrono::Utc::now()).await.unwrap();
    assert!(
        !due3.iter().any(|t| t.id == id),
        "disabled trigger should not be due"
    );

    // delete -> get returns None
    store.delete(id, eng).await.unwrap();
    let gone = store.get(id, eng).await.unwrap();
    assert!(gone.is_none(), "deleted trigger should not be found");
}

// ---------------------------------------------------------------------------
// (d) poll_one fires a 'connector' run + advances cursor
// ---------------------------------------------------------------------------

/// A no-op ProviderKeysStore: the http-api connector doesn't require an API
/// key (it's optional), so we return None for every lookup.
struct NoopProviderKeys;

#[async_trait]
impl celerex_api::providers::store::ProviderKeysStore for NoopProviderKeys {
    async fn set(
        &self,
        _engagement_id: Uuid,
        _provider_id: &str,
        _plaintext_key: &str,
    ) -> Result<celerex_api::providers::store::ProviderKeyMeta, String> {
        Err("noop".into())
    }
    async fn get(
        &self,
        _engagement_id: Uuid,
        _provider_id: &str,
    ) -> Result<Option<String>, String> {
        Ok(None)
    }
    async fn list_for_engagement(
        &self,
        _engagement_id: Uuid,
    ) -> Result<Vec<celerex_api::providers::store::ProviderKeyMeta>, String> {
        Ok(vec![])
    }
    async fn delete(&self, _engagement_id: Uuid, _provider_id: &str) -> Result<bool, String> {
        Ok(false)
    }
}

/// A no-op executor Handler that succeeds immediately for any execution type.
struct NoopHandler;

#[async_trait]
impl celerex_orchestrator::tool_dispatch::Handler for NoopHandler {
    async fn invoke(
        &self,
        _config: serde_json::Value,
        _inputs: serde_json::Value,
    ) -> Result<serde_json::Value, celerex_orchestrator::tool_dispatch::DispatchError> {
        Ok(serde_json::json!({}))
    }
}

#[tokio::test]
async fn poll_one_fires_connector_run_and_advances_cursor() {
    use celerex_api::automations::poll::AutomationPollLoop;
    use celerex_api::automations::store::{
        AutomationTrigger, AutomationTriggersStore, PgAutomationTriggersStore,
    };
    use celerex_api::project_members::store::{MembersStore, PgMembersStore};
    use celerex_api::runs::PgRunStore;
    use celerex_api::runs::bus::RunBus;
    use celerex_orchestrator::execution_type::ExecutionType;
    use celerex_orchestrator::executor::Executor;
    use celerex_orchestrator::tool_dispatch::Dispatcher;
    use std::sync::Arc;
    use wiremock::matchers::method;
    use wiremock::{Mock, MockServer, ResponseTemplate};

    let db = common::start_test_db().await;

    // ---- Seed a member with an empty graph -----------------------------------
    let (eng, _project, member_id) = seed_member(&db.pool).await;

    // ---- Wiremock: 2 items --------------------------------------------------
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "items": [
                {"id": "1", "subject": "first"},
                {"id": "2", "subject": "second"}
            ]
        })))
        .mount(&server)
        .await;

    // ---- Seed automation_triggers row ---------------------------------------
    let trigger_store = PgAutomationTriggersStore::new(db.pool.clone());
    let trigger_id = trigger_store
        .create(AutomationTrigger {
            id: Uuid::new_v4(),
            project_member_id: member_id,
            connector_id: "http-api".into(),
            source_config: serde_json::json!({
                "url": server.uri(),
                "items_path": "items",
                "id_field": "id"
            }),
            filter: None,
            poll_interval_secs: 60,
            cursor: None,
            last_polled_at: None,
            enabled: true,
        })
        .await
        .unwrap();

    // ---- Build Executor (same pattern as scheduler_dispatch.rs) -------------
    // Register a NoopHandler for every execution type the member graph might
    // use. Our empty graph has no nodes, so the executor returns Succeeded
    // immediately — but we still need a valid Dispatcher.
    let mut dispatcher = Dispatcher::new();
    for ty in [
        ExecutionType::Script,
        ExecutionType::Http,
        ExecutionType::Llm,
        ExecutionType::Doc,
    ] {
        dispatcher.register(ty, Arc::new(NoopHandler));
    }
    let run_bus = Arc::new(RunBus::new());
    let run_store = Arc::new(PgRunStore::with_bus(db.pool.clone(), run_bus));
    let executor = Arc::new(Executor::new(Arc::new(dispatcher), run_store));

    // ---- Build the remaining collaborators ----------------------------------
    let members: Arc<dyn MembersStore> = Arc::new(PgMembersStore::new(db.pool.clone()));
    let notifiers: Arc<Vec<Arc<dyn celerex_api::notifiers::Notifier>>> = Arc::new(vec![Arc::new(
        celerex_api::notifiers::console::ConsoleNotifier,
    )]);
    let provider_keys: Arc<dyn celerex_api::providers::store::ProviderKeysStore> =
        Arc::new(NoopProviderKeys);

    // ---- Build AutomationPollLoop and call poll_one -------------------------
    let poll_loop = AutomationPollLoop {
        store: Arc::new(trigger_store),
        members: members.clone(),
        provider_keys,
        pool: db.pool.clone(),
        executor,
        notifiers,
        tick: std::time::Duration::from_secs(60), // not used; we call poll_one directly
    };

    let trigger = poll_loop.store.get(trigger_id, eng).await.unwrap().unwrap();
    let now = chrono::Utc::now();
    poll_loop.poll_one(&trigger, now).await;

    // ---- Assert: 2 runs rows with triggered_by='connector' ------------------
    // poll_one spawns a tokio task per enqueue_run; give it a moment to land.
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    let mut run_count: i64 = 0;
    while std::time::Instant::now() < deadline {
        run_count = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM runs WHERE project_member_id = $1 AND triggered_by = 'connector'",
        )
        .bind(member_id)
        .fetch_one(&db.pool)
        .await
        .unwrap();
        if run_count >= 2 {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    assert_eq!(
        run_count, 2,
        "2 connector runs must be enqueued (one per item)"
    );

    // ---- Assert: cursor advanced to '2' (max item id) -----------------------
    let cursor: Option<String> =
        sqlx::query_scalar("SELECT cursor FROM automation_triggers WHERE id = $1")
            .bind(trigger_id)
            .fetch_one(&db.pool)
            .await
            .unwrap();
    assert_eq!(
        cursor.as_deref(),
        Some("2"),
        "cursor must advance to the max item id"
    );
}
