pub mod assistant;
pub mod automations;
pub mod connectors;
pub mod db;
pub mod engagements;
pub mod galvo_hub_routes;
pub mod auth;
pub mod projects;
pub mod docs;
pub mod metrics;
pub mod plugins;
pub mod design;
pub mod providers;
pub mod reviews;
pub mod webhooks;
pub mod runs;
pub mod llm;
pub mod scheduler;
pub mod project_members;
pub mod notifiers;
pub mod settings;

use std::sync::Arc;
use axum::{Router, routing::{any, get, patch, post, put}, Json, extract::State};
use serde_json::json;
use sqlx::PgPool;
use tower_http::cors::{Any, CorsLayer};
use tower_http::services::{ServeDir, ServeFile};

use crate::plugins::credentials::{CredentialStore, LocalAeadStore};
use crate::project_members::store::{MembersStore, PgMembersStore};
use crate::providers::store::{PgProviderKeysStore, ProviderKeysStore};
use crate::runs::bus::RunBus;
use celerex_orchestrator::{
    execution_type::ExecutionType,
    executor::Executor,
    handlers::{http::HttpHandler, input::InputHandler, llm::LlmHandler, script::ScriptHandler},
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
    /// Per-engagement encrypted API keys for LLM providers. Phase 1: storage
    /// + retrieval. Actual override-at-call-time wiring into the LlmHandler
    /// is a follow-up.
    pub provider_keys: Arc<dyn ProviderKeysStore>,
    /// Pending provider-plane design-chat runs, keyed by run id. `POST /api/runs`
    /// registers; `GET /api/runs/{id}/events` consumes (single-shot). See
    /// `runs::design_chat`.
    pub design_runs: crate::runs::design_chat::DesignRuns,
    /// Engagements permitted to drive Galvo Hub federation (register/invoke/
    /// status). Parsed once at boot from `CELEREX_HUB_ADMIN_ENGAGEMENTS`;
    /// empty/unset = deny all.
    pub hub_admin_engagements: std::collections::HashSet<uuid::Uuid>,
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

/// Build a `PgProviderKeysStore` from `CELEREX_CRED_KEY`. Sharing the same
/// envelope key as the plugin credential store keeps key management simple
/// for Phase 1 — both encrypt-at-rest stores rotate together.
fn build_provider_keys_store(pool: PgPool) -> Arc<dyn ProviderKeysStore> {
    Arc::new(
        PgProviderKeysStore::from_env(pool)
            .expect("CELEREX_CRED_KEY must be set for provider_keys store"),
    )
}

/// Serve the OmniVyx Design static export (the one client) from the same server
/// as the API — this is the "one server, one service" half. The directory is the
/// Designer's `next build` (`output: 'export'`) output; `CELEREX_WEB_DIR` points at
/// it (baked into the image, or a local `out/`). Any non-API path falls back to
/// `index.html` so the SPA's client routes (e.g. `/callback`) resolve. When the
/// dir is absent (e.g. `cargo run` with no built frontend) the API still serves;
/// the SPA paths just 404.
fn web_service() -> ServeDir<ServeFile> {
    let dir = std::env::var("CELEREX_WEB_DIR").unwrap_or_else(|_| "web".to_string());
    let index = format!("{}/index.html", dir.trim_end_matches('/'));
    ServeDir::new(&dir).fallback(ServeFile::new(index))
}

fn route_chain() -> Router<AppState> {
    Router::new()
        .route("/api/health", get(|| async { Json(json!({ "ok": true })) }))
        .route("/api/auth/login", post(auth::routes::login))
        .route("/api/auth/signup", post(auth::routes::signup))
        .route("/api/auth/workos", post(auth::routes::workos_login))
        .route("/api/auth/workos/exchange", post(auth::routes::workos_exchange))
        .route("/api/engagements", get(engagements::list).post(engagements::create))
        .route("/api/projects", get(projects::list).post(projects::create))
        .route("/api/docs", post(docs::create))
        .route("/api/engagements/{engagement_id}/docs", get(docs::list_for_engagement))
        .route("/api/docs/{id}", get(docs::get).delete(docs::delete))
        .route("/api/docs/{id}/blocks", get(docs::list_blocks).post(docs::append_block))
        .route("/api/docs/{doc_id}/blocks/{block_id}", patch(docs::replace_block))
        .route("/api/plugins", get(plugins::routes::list).post(plugins::routes::install))
        .route("/api/plugins/{id}/credentials", put(plugins::routes::upsert_credentials))
        .route("/api/webhooks/{token}/{event}", post(webhooks::ingest))
        .route("/api/project_members", post(project_members::routes::create))
        .route(
            "/api/project_members/fleet",
            get(project_members::routes::fleet),
        )
        .route(
            "/api/project_members/{id}",
            get(project_members::routes::get)
                .put(project_members::routes::update)
                .delete(project_members::routes::delete),
        )
        .route("/api/projects/{project_id}/members", get(project_members::routes::list))
        .route("/api/project_members/{id}/run", post(project_members::routes::run_manual))
        .route("/api/runs", get(runs::routes::list).post(runs::design_chat::create))
        .route("/api/runs/recent", get(runs::routes::recent))
        .route("/api/runs/{run_id}/steps", get(runs::routes::steps))
        .route("/api/runs/{run_id}/events", get(runs::sse::stream))
        .route("/api/approvals", get(reviews::list_approvals))
        .route("/api/runs/{run_id}/review", post(reviews::submit_review))
        .route("/api/design/ingest", post(design::ingest))
        .route("/api/design/templates", get(design::list_templates))
        .route("/api/design/templates/{id}", get(design::get_template))
        .route("/api/design/assets/{id}", get(design::get_asset))
        .route("/api/design/providers", get(design::list_design_providers))
        .route("/api/design/llm/{provider}/{*rest}", any(design::proxy::proxy))
        .route("/api/design/providers/{id}/key", get(design::copy_provider_key))
        .route("/api/design/rituals", post(design::rituals::create_ritual).get(design::rituals::list_rituals))
        .route("/api/design/rituals/{id}", axum::routing::delete(design::rituals::delete_ritual).put(design::rituals::update_ritual))
        .route("/api/design/rituals/{id}/run", post(design::rituals::run_ritual_now))
        .route("/api/design/rituals/{id}/runs", get(design::rituals::list_ritual_runs))
        .route("/api/design/automations", post(design::automations::create_automation).get(design::automations::list_automations))
        .route("/api/design/automations/{id}", axum::routing::delete(design::automations::delete_automation).put(design::automations::update_automation))
        .route("/api/design/automations/{id}/run", post(design::automations::run_automation_now))
        .route("/api/design/automations/{id}/runs", get(design::automations::list_automation_runs))
        .route("/api/design/deploy", post(design::deploy::create_deploy))
        .route("/api/design/deploy/recommend", post(design::deploy::recommend))
        .route("/api/design/deploy/credential", put(design::deploy::set_deploy_credential))
        .route("/api/design/deploy/credential/{target}", get(design::deploy::get_deploy_credential_status))
        .route("/api/design/deploy/status/{deployment_id}", get(design::deploy::get_deploy_status))
        .route("/api/design/templates/{template_id}/slots/{slot}/instruction", put(design::living::set_instruction))
        .route("/api/design/templates/{template_id}/slots", get(design::living::list_slots))
        .route("/api/design/templates/{template_id}/slots/{slot}/fill", post(design::living::fill_slot))
        .route("/api/design/templates/{template_id}/fill", post(design::living::fill_all))
        .route("/api/design/templates/{template_id}/render", get(design::living::render))
        .route("/api/design/templates/{template_id}/schedule", post(design::living::schedule))
        .route("/api/design/connectors", get(design::list_design_connectors))
        .route("/api/design/connectors/{id}/credential", get(design::get_connector_credential).put(design::set_connector_credential))
        .route("/api/design/copilot/login/start", post(design::copilot::start))
        .route("/api/design/copilot/login/status", get(design::copilot::status))
        .route("/api/design/codex/credential", post(design::codex::put_credential))
        .route("/api/design/codex/status", get(design::codex::status))
        .route("/api/design/xai/login/start", post(design::xai::start))
        .route("/api/design/xai/login/status", get(design::xai::status))
        .route("/api/design/tokens", post(design::mint_token))
        .route("/api/design/pair/start", post(design::pair_start))
        .route("/api/design/pair/approve", post(design::pair_approve))
        .route("/api/design/pair/poll", post(design::pair_poll))
        .route("/api/galvo/hub/register", post(galvo_hub_routes::register))
        .route("/api/galvo/hub/status", get(galvo_hub_routes::status))
        .route("/api/galvo/hub/invoke", post(galvo_hub_routes::invoke))
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
        .route("/api/providers/keys", get(providers::routes::list))
        .route(
            "/api/providers/keys/{provider_id}",
            put(providers::routes::set_key).delete(providers::routes::delete_key),
        )
        .route("/api/assistant/chat", post(assistant::chat))
        .route("/api/assistant/chat/stream", post(assistant::chat_stream))
        .route("/api/metrics/summary", get(metrics::summary))
        .route("/api/settings", get(settings::get_settings).put(settings::put_settings))
        // Everything that isn't an API route is the OmniVyx Design SPA — one server.
        .fallback_service(web_service())
}

pub fn build_app_with_store(pool: PgPool, credentials: Arc<dyn CredentialStore>) -> Router {
    // Fail fast at boot: panics with a clear message if CELEREX_JWT_SECRET is unset/empty in production.
    let _ = crate::auth::jwt::jwt_secret();
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
    // Engagement-aware LLM handler: prefers a per-engagement decrypted key for
    // an explicitly-named provider, falling back to the house registry. If the
    // key store can't be built (no CELEREX_CRED_KEY), fall back to the plain
    // env-only handler so the app still starts.
    match crate::providers::store::PgProviderKeysStore::from_env(pool.clone()) {
        Ok(keys) => dispatcher.register(
            ExecutionType::Llm,
            Arc::new(crate::llm::handler::EngagementLlmHandler::new(
                registry_arc.clone(),
                Arc::new(keys),
            )),
        ),
        Err(_) => dispatcher.register(
            ExecutionType::Llm,
            Arc::new(LlmHandler { registry: registry_arc.clone() }),
        ),
    }
    // Full production handler set so every node type a flow can contain actually
    // executes. (E2E tests inject their own dispatcher + fixtures as needed.)
    //   Input  — trigger / seed nodes echo a payload downstream.
    //   Http   — outbound HTTP / plugin connectors.
    //   Script — local command execution.
    //   Doc    — read/write documents (the "doc builder" backbone).
    dispatcher.register(ExecutionType::Input, Arc::new(InputHandler));
    dispatcher.register(ExecutionType::Http, Arc::new(HttpHandler::new()));
    dispatcher.register(ExecutionType::Script, Arc::new(ScriptHandler));
    dispatcher.register(
        ExecutionType::Doc,
        Arc::new(crate::docs::handler::DocHandler::new(pool.clone())),
    );
    // External connectors / federation: galvo-memory, Galvo agents, Microsoft 365.
    dispatcher.register(
        ExecutionType::Memory,
        Arc::new(crate::connectors::memory::MemoryHandler::new()),
    );
    dispatcher.register(
        ExecutionType::Galvo,
        Arc::new(crate::connectors::galvo::GalvoHandler::new()),
    );
    dispatcher.register(
        ExecutionType::M365,
        Arc::new(crate::connectors::m365::M365Handler::new()),
    );
    // Design handoff: render a Celerex Design template's slots with flow data.
    dispatcher.register(
        ExecutionType::Render,
        Arc::new(celerex_orchestrator::handlers::render::RenderHandler),
    );
    // Living Documents v2: the scaffold's `fill` node fills a template's
    // Living-Doc slots (engagement-key aware, like the v1 fill endpoint). Only
    // registered when the key store builds (needs CELEREX_CRED_KEY); without it
    // a `slot_fill` node errors as "no handler", same posture as the LLM handler.
    if let Ok(keys) = crate::providers::store::PgProviderKeysStore::from_env(pool.clone()) {
        let provider_keys: Arc<dyn crate::providers::store::ProviderKeysStore> = Arc::new(keys);
        dispatcher.register(
            ExecutionType::SlotFill,
            Arc::new(crate::design::slot_fill_handler::SlotFillHandler::new(
                pool.clone(),
                provider_keys,
                registry_arc.clone(),
            )),
        );
    }
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

    let provider_keys = build_provider_keys_store(pool.clone());

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

    if spawn_scheduler {
        let triggers: Arc<dyn automations::store::AutomationTriggersStore> =
            Arc::new(automations::store::PgAutomationTriggersStore::new(pool.clone()));
        automations::poll::AutomationPollLoop {
            store: triggers,
            members: members.clone(),
            provider_keys: provider_keys.clone(),
            pool: pool.clone(),
            executor: executor.clone(),
            notifiers: notifiers.clone(),
            tick: std::time::Duration::from_secs(30),
        }
        .spawn();
    }
    // CORS allowlist: origins are read from CELEREX_ALLOWED_ORIGINS (comma-separated).
    // Falls back to localhost dev defaults when the var is unset/empty.
    // `allow_methods`/`allow_headers` stay permissive — auth is Bearer (no
    // ambient credentials), so only the origin requires explicit enumeration.
    let cors = CorsLayer::new()
        .allow_origin(tower_http::cors::AllowOrigin::list(allowed_origins()))
        .allow_methods(Any)
        .allow_headers(Any);
    route_chain()
        .with_state(AppState {
            pool,
            credentials,
            executor,
            members,
            run_bus,
            notifiers,
            providers: Arc::new(provider_ids),
            providers_registry,
            provider_keys,
            design_runs: runs::design_chat::new_registry(),
            hub_admin_engagements: hub_admin_engagements(),
        })
        .layer(cors)
}

/// Parse CELEREX_ALLOWED_ORIGINS (comma-separated) into explicit origins.
/// Empty/unset → dev defaults. Never returns `Any` (public sites must not be open).
fn allowed_origins() -> Vec<axum::http::HeaderValue> {
    let raw = std::env::var("CELEREX_ALLOWED_ORIGINS").unwrap_or_default();
    let list: Vec<String> = raw.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect();
    let defaults = ["http://localhost:5173", "http://localhost:3001"];
    let chosen: Vec<String> = if list.is_empty() {
        defaults.iter().map(|s| s.to_string()).collect()
    } else {
        list
    };
    chosen.iter().filter_map(|o| o.parse::<axum::http::HeaderValue>().ok()).collect()
}

/// Parse CELEREX_HUB_ADMIN_ENGAGEMENTS (comma-separated engagement UUIDs) into
/// the set of engagements allowed to drive Galvo Hub federation. Malformed
/// entries are skipped with a warning. Empty/unset → empty set → deny all.
fn hub_admin_engagements() -> std::collections::HashSet<uuid::Uuid> {
    std::env::var("CELEREX_HUB_ADMIN_ENGAGEMENTS")
        .unwrap_or_default()
        .split(',')
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .filter_map(|s| match uuid::Uuid::parse_str(s) {
            Ok(u) => Some(u),
            Err(_) => {
                tracing::warn!(value = %s, "ignoring malformed CELEREX_HUB_ADMIN_ENGAGEMENTS entry");
                None
            }
        })
        .collect()
}

#[cfg(test)]
mod cors_tests {
    use super::*;
    #[test]
    fn parses_comma_list_and_defaults() {
        // SAFETY: single-process test.
        unsafe { std::env::set_var("CELEREX_ALLOWED_ORIGINS", "https://designer.ailuxis.com, https://x.test"); }
        let v = allowed_origins();
        assert_eq!(v.len(), 2);
        assert!(v.iter().any(|h| h.to_str().unwrap() == "https://designer.ailuxis.com"));
        unsafe { std::env::set_var("CELEREX_ALLOWED_ORIGINS", ""); }
        assert_eq!(allowed_origins().len(), 2); // dev defaults
    }
}
