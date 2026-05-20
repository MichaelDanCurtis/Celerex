pub mod assistant;
pub mod db;
pub mod engagements;
pub mod auth;
pub mod projects;
pub mod docs;
pub mod plugins;
pub mod webhooks;
pub mod runs;
pub mod llm;
pub mod scheduler;
pub mod project_members;
pub mod notifiers;

use std::sync::Arc;
use axum::{Router, routing::{get, patch, post, put}, Json, extract::State};
use serde_json::json;
use sqlx::PgPool;

use crate::plugins::credentials::{CredentialStore, LocalAeadStore};
use crate::project_members::store::{MembersStore, PgMembersStore};
use crate::runs::bus::RunBus;
use celerex_orchestrator::{
    execution_type::ExecutionType,
    executor::Executor,
    handlers::llm::LlmHandler,
    llm::{Provider, ProviderRegistry},
    tool_dispatch::Dispatcher,
};
use crate::llm::{
    anthropic::AnthropicClient, gemini::GeminiProvider, kimi::KimiProvider, openai::OpenAiProvider,
};

#[derive(Clone)]
pub struct AppState {
    pub pool: PgPool,
    pub credentials: Arc<dyn CredentialStore>,
    pub executor: Arc<Executor>,
    pub members: Arc<dyn MembersStore>,
    pub run_bus: Arc<RunBus>,
    pub notifiers: Arc<Vec<Arc<dyn notifiers::Notifier>>>,
    pub providers: Arc<Vec<String>>,
    pub providers_registry: Arc<ProviderRegistry>,
}

async fn list_providers(State(state): State<AppState>) -> Json<serde_json::Value> {
    Json(json!({ "providers": &*state.providers }))
}

pub fn build_app(pool: PgPool) -> Router {
    let store: Arc<dyn CredentialStore> = Arc::new(
        LocalAeadStore::from_env().expect("CELEREX_CRED_KEY must be set")
    );
    build_app_with_store(pool, store)
}

fn route_chain() -> Router<AppState> {
    Router::new()
        .route("/api/health", get(|| async { Json(json!({ "ok": true })) }))
        .route("/api/engagements", get(engagements::list).post(engagements::create))
        .route("/api/projects", get(projects::list).post(projects::create))
        .route("/api/docs", post(docs::create))
        .route("/api/docs/{id}/blocks", post(docs::append_block))
        .route("/api/docs/{doc_id}/blocks/{block_id}", patch(docs::replace_block))
        .route("/api/plugins", get(plugins::routes::list).post(plugins::routes::install))
        .route("/api/plugins/{id}/credentials", put(plugins::routes::upsert_credentials))
        .route("/api/webhooks/{plugin_id}/{event}", post(webhooks::ingest))
        .route("/api/project_members", post(project_members::routes::create))
        .route(
            "/api/project_members/fleet",
            get(project_members::routes::fleet),
        )
        .route(
            "/api/project_members/{id}",
            get(project_members::routes::get).put(project_members::routes::update),
        )
        .route("/api/projects/{project_id}/members", get(project_members::routes::list))
        .route("/api/project_members/{id}/run", post(project_members::routes::run_manual))
        .route("/api/runs/{run_id}/events", get(runs::sse::stream))
        .route(
            "/api/schedules",
            post(scheduler::routes::create).get(scheduler::routes::list),
        )
        .route(
            "/api/schedules/{id}",
            put(scheduler::routes::update).delete(scheduler::routes::delete),
        )
        .route("/api/schedules/{id}/fire", post(scheduler::routes::fire))
        .route("/api/providers", get(list_providers))
        .route("/api/assistant/chat", post(assistant::chat))
}

pub fn build_app_with_store(pool: PgPool, credentials: Arc<dyn CredentialStore>) -> Router {
    let mut dispatcher = Dispatcher::new();
    let mut registry = ProviderRegistry::new();
    let mut provider_ids: Vec<String> = Vec::new();
    if let Ok(client) = AnthropicClient::from_env() {
        let provider: Arc<dyn Provider> = Arc::new(client);
        registry.register("anthropic", provider.clone());
        registry.register("default", provider);
        provider_ids.push("anthropic".to_string());
        provider_ids.push("default".to_string());
    }
    if let Ok(provider) = OpenAiProvider::from_env() {
        registry.register("openai", Arc::new(provider));
        provider_ids.push("openai".to_string());
    }
    if let Ok(provider) = KimiProvider::from_env() {
        registry.register("kimi", Arc::new(provider));
        provider_ids.push("kimi".to_string());
    }
    if let Ok(provider) = GeminiProvider::from_env() {
        registry.register("gemini", Arc::new(provider));
        provider_ids.push("gemini".to_string());
    }
    let registry_arc = Arc::new(registry);
    dispatcher.register(
        ExecutionType::Llm,
        Arc::new(LlmHandler { registry: registry_arc.clone() }),
    );
    build_app_with_dispatcher_inner(
        pool,
        credentials,
        Arc::new(dispatcher),
        true,
        provider_ids,
        registry_arc,
    )
}

/// Build a router with a preconfigured dispatcher (for E2E tests that register
/// fixture handlers). Skips the scheduler spawn — tests drive runs manually.
pub fn build_app_with_dispatcher(
    pool: PgPool,
    credentials: Arc<dyn CredentialStore>,
    dispatcher: Arc<Dispatcher>,
) -> Router {
    build_app_with_dispatcher_inner(
        pool,
        credentials,
        dispatcher,
        false,
        Vec::new(),
        Arc::new(ProviderRegistry::new()),
    )
}

fn build_app_with_dispatcher_inner(
    pool: PgPool,
    credentials: Arc<dyn CredentialStore>,
    dispatcher: Arc<Dispatcher>,
    spawn_scheduler: bool,
    provider_ids: Vec<String>,
    providers_registry: Arc<ProviderRegistry>,
) -> Router {
    let run_bus = Arc::new(RunBus::new());
    let run_store = Arc::new(runs::PgRunStore::with_bus(pool.clone(), run_bus.clone()));
    let executor = Arc::new(Executor::new(dispatcher, run_store));
    let members: Arc<dyn MembersStore> = Arc::new(PgMembersStore::new(pool.clone()));
    let notifiers: Arc<Vec<Arc<dyn notifiers::Notifier>>> = Arc::new(vec![
        Arc::new(notifiers::console::ConsoleNotifier),
        Arc::new(notifiers::teams_stub::StubTeamsNotifier::default()),
    ]);

    if spawn_scheduler {
        let schedules: Arc<dyn scheduler::store::SchedulesStore> =
            Arc::new(scheduler::store::PgSchedulesStore::new(pool.clone()));
        let pool_for_sched = pool.clone();
        let members_for_sched = members.clone();
        let executor_for_sched = executor.clone();
        let notifiers_for_sched = notifiers.clone();
        let dispatch = Arc::new(move |s: scheduler::store::Schedule| {
            let pool = pool_for_sched.clone();
            let members = members_for_sched.clone();
            let executor = executor_for_sched.clone();
            let notifiers = notifiers_for_sched.clone();
            tokio::spawn(async move {
                let engagement_id = match members.engagement_id_for(s.project_member_id).await {
                    Ok(v) => v,
                    Err(e) => {
                        tracing::error!(
                            member=%s.project_member_id,
                            error=%e,
                            "scheduler: failed to resolve engagement_id"
                        );
                        return;
                    }
                };
                if let Err(e) = project_members::run_job::enqueue_run(
                    pool,
                    members,
                    executor,
                    notifiers,
                    s.project_member_id,
                    engagement_id,
                    project_members::run_job::TRIGGERED_BY_SCHEDULED,
                )
                .await
                {
                    tracing::error!(
                        member=%s.project_member_id,
                        error=%e,
                        "scheduler: enqueue_run failed"
                    );
                }
            })
        });
        scheduler::r#loop::SchedulerLoop {
            store: schedules,
            tick: std::time::Duration::from_secs(30),
            dispatch,
        }
        .spawn();
    }

    route_chain().with_state(AppState {
        pool,
        credentials,
        executor,
        members,
        run_bus,
        notifiers,
        providers: Arc::new(provider_ids),
        providers_registry,
    })
}
