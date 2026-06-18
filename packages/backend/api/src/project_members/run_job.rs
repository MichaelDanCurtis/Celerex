use std::sync::Arc;

use serde_json::json;
use sqlx::PgPool;
use uuid::Uuid;

use crate::notifiers::{NotificationEvent, Notifier};
use crate::project_members::graph;
use crate::project_members::store::MembersStore;
use celerex_orchestrator::executor::Executor;
use celerex_orchestrator::job::{JobRequest, RunStatus};

/// Trigger source recorded in `runs.triggered_by`.
/// Keep in sync with the CHECK constraint in migration 0009_amend_runs.sql.
pub const TRIGGERED_BY_MANUAL: &str = "manual";
pub const TRIGGERED_BY_SCHEDULED: &str = "scheduled";
pub const TRIGGERED_BY_CONNECTOR: &str = "connector";

/// Default model for reasoning members (Project Members — Sonnet-class).
pub const DEFAULT_REASONING_MODEL: &str = "claude-sonnet-4-6";
/// Default model for writer members (Document Servicers — Haiku-class).
pub const DEFAULT_WRITER_MODEL: &str = "claude-haiku-4-5";
/// Default provider id (resolved by the registry — Anthropic in production
/// when ANTHROPIC_API_KEY is set).
pub const DEFAULT_PROVIDER_ID: &str = "default";

/// Walk a member graph and fill in agent-type-aware defaults on LLM nodes.
///
/// For every node where `config.execution == "llm"`:
///   * if `config.model` is missing or null, set it to the Haiku-class default
///     for writers and the Sonnet-class default for reasoning members;
///   * if `config.provider_id` is missing or null, set it to `"default"` so
///     the registry resolves to the configured house provider.
///
/// Values the user explicitly set in the graph are NEVER overwritten.
/// Malformed graphs (missing `nodes`, non-object configs) are silently skipped
/// — the executor will reject them later with a clearer error.
pub fn apply_agent_type_defaults(graph: &mut serde_json::Value, agent_type: &str) {
    let default_model = match agent_type {
        "writer" => DEFAULT_WRITER_MODEL,
        _ => DEFAULT_REASONING_MODEL,
    };
    let Some(nodes) = graph.get_mut("nodes").and_then(|n| n.as_array_mut()) else {
        return;
    };
    for node in nodes {
        let is_llm = node
            .get("config")
            .and_then(|c| c.get("execution"))
            .and_then(|e| e.as_str())
            .map(|s| s == "llm")
            .unwrap_or(false);
        if !is_llm {
            continue;
        }
        let Some(cfg) = node.get_mut("config").and_then(|c| c.as_object_mut()) else {
            continue;
        };
        let needs_model = cfg
            .get("model")
            .map(|v| v.is_null())
            .unwrap_or(true);
        if needs_model {
            cfg.insert("model".to_string(), serde_json::json!(default_model));
        }
        let needs_provider = cfg
            .get("provider_id")
            .map(|v| v.is_null())
            .unwrap_or(true);
        if needs_provider {
            cfg.insert(
                "provider_id".to_string(),
                serde_json::json!(DEFAULT_PROVIDER_ID),
            );
        }
    }
}

/// Inject the run's `engagement_id` into nodes that need tenant context.
///
/// Handlers only see node `config` + `inputs` (not run context). `doc` create
/// ops need the tenant id to place the document; `llm` nodes need it so the
/// engagement-aware handler can resolve a per-engagement provider key; `memory`
/// nodes need it to scope store/search to the tenant (no cross-tenant recall).
/// The id is security-relevant: we ALWAYS overwrite it with the run's true
/// engagement so a flow author can't pin a foreign engagement_id and cross tenants.
pub fn apply_doc_engagement(graph: &mut serde_json::Value, engagement_id: Uuid) {
    let Some(nodes) = graph.get_mut("nodes").and_then(|n| n.as_array_mut()) else {
        return;
    };
    for node in nodes {
        let exec = node
            .get("config")
            .and_then(|c| c.get("execution"))
            .and_then(|e| e.as_str());
        if exec != Some("doc") && exec != Some("llm") && exec != Some("memory") {
            continue;
        }
        let Some(cfg) = node.get_mut("config").and_then(|c| c.as_object_mut()) else {
            continue;
        };
        // Always overwrite with the run's true engagement — never trust an
        // engagement_id the flow author may have pinned into the node config.
        cfg.insert(
            "engagement_id".to_string(),
            json!(engagement_id.to_string()),
        );
    }
}

/// Shared pipeline for starting a run: lookup member, snapshot, compile graph,
/// INSERT `runs` row, spawn the executor + notifier task.
///
/// Used by both the manual `POST /run` route and the scheduler dispatch closure.
pub async fn enqueue_run(
    pool: PgPool,
    members: Arc<dyn MembersStore>,
    executor: Arc<Executor>,
    notifiers: Arc<Vec<Arc<dyn Notifier>>>,
    member_id: Uuid,
    engagement_id: Uuid,
    triggered_by: &'static str,
) -> Result<Uuid, String> {
    // `engagement_id` here is the member's TRUE owning engagement, already
    // resolved by every caller (manual route resolves via engagement_id_for and
    // passes it; the scheduler/automation background ticks resolve per-row and
    // pass it). Scoping the read to it is correct for both request and sweep
    // paths — it never narrows to a wrong engagement.
    let member = members
        .get(member_id, engagement_id)
        .await?
        .ok_or_else(|| format!("member {member_id} not found"))?;
    // A member created out-of-band (seeded directly, imported, etc.) may have no
    // snapshot row yet. The `runs.snapshot_id` column is nullable, so pin NULL
    // rather than hard-failing the whole run — the run still executes against the
    // member's current graph.
    let snap_id: Option<Uuid> = members.snapshot_id_for_current(member_id).await.ok();
    let mut graph = member.graph_json.clone();
    apply_agent_type_defaults(&mut graph, &member.agent_type);
    apply_doc_engagement(&mut graph, engagement_id);
    let steps = graph::compile(&graph)?;

    let run_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO runs (id, engagement_id, project_id, project_member_id, trigger, status, snapshot_id, triggered_by)
         VALUES ($1, $2, $3, $4, $5::jsonb, 'running', $6, $7)",
    )
    .bind(run_id)
    .bind(engagement_id)
    .bind(Some(member.project_id))
    .bind(member_id)
    .bind(json!({"source": triggered_by, "project_member_id": member_id}).to_string())
    .bind(snap_id)
    .bind(triggered_by)
    .execute(&pool)
    .await
    .map_err(|e| e.to_string())?;

    let req = JobRequest {
        run_id,
        engagement_id,
        project_id: Some(member.project_id),
        steps,
    };
    let flow_name = member.name.clone();
    let exec = executor.clone();
    let pool_for_task = pool.clone();
    let notifiers_for_task = notifiers.clone();
    tokio::spawn(async move {
        let status = exec.run(req.clone()).await;
        // Best-effort: record this run as a galvo-memory :Session node so the
        // Celerex workspace accrues memory of what it has run.
        let outcome = match &status {
            Ok(RunStatus::Succeeded) => "success",
            Ok(RunStatus::Failed) => "failed",
            Ok(RunStatus::AwaitingReview) => "awaiting_review",
            _ => "abandoned",
        };
        crate::connectors::memory::log_run_session(flow_name, engagement_id, req.run_id, outcome);

        // A paused run isn't terminal: stamp `awaiting_review` (the executor leaves
        // the row at `running`) and emit no terminal notification — the run resumes
        // when a human approves/rejects via the approvals API.
        if matches!(status, Ok(RunStatus::AwaitingReview)) {
            let _ = sqlx::query("UPDATE runs SET status = 'awaiting_review' WHERE id = $1")
                .bind(req.run_id)
                .execute(&pool_for_task)
                .await;
        }
        let event = match status {
            Ok(RunStatus::Failed) => Some(NotificationEvent::RunFailed {
                run_id: req.run_id,
                project_member_id: member_id,
                step_id: "unknown".into(),
                error: "run failed; see run_steps for details".into(),
            }),
            Ok(RunStatus::Succeeded) => Some(NotificationEvent::RunSucceeded {
                run_id: req.run_id,
                project_member_id: member_id,
            }),
            _ => None,
        };
        if let Some(ev) = event {
            for n in notifiers_for_task.iter() {
                let _ = n.dispatch(&ev).await;
            }
            let _ = sqlx::query(
                "INSERT INTO notifications (project_member_id, run_id, event, payload_json)
                 VALUES ($1, $2, $3, $4)",
            )
            .bind(ev.project_member_id())
            .bind(ev.run_id())
            .bind(ev.tag())
            .bind(ev.payload())
            .execute(&pool_for_task)
            .await;
            let status_str = match ev {
                NotificationEvent::RunFailed { .. } => "failed",
                NotificationEvent::RunSucceeded { .. } => "succeeded",
            };
            let _ = sqlx::query("UPDATE runs SET status = $2 WHERE id = $1")
                .bind(req.run_id)
                .bind(status_str)
                .execute(&pool_for_task)
                .await;
        }
    });

    Ok(run_id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn writer_gets_haiku_default_for_llm_nodes() {
        let mut g = json!({
            "nodes": [
                {"id": "a", "type": "action", "config": {"execution": "llm"}}
            ],
            "edges": []
        });
        apply_agent_type_defaults(&mut g, "writer");
        assert_eq!(g["nodes"][0]["config"]["model"], "claude-haiku-4-5");
        assert_eq!(g["nodes"][0]["config"]["provider_id"], "default");
    }

    #[test]
    fn reasoning_gets_sonnet_default_for_llm_nodes() {
        let mut g = json!({
            "nodes": [
                {"id": "a", "type": "action", "config": {"execution": "llm"}}
            ],
            "edges": []
        });
        apply_agent_type_defaults(&mut g, "reasoning");
        assert_eq!(g["nodes"][0]["config"]["model"], "claude-sonnet-4-6");
        assert_eq!(g["nodes"][0]["config"]["provider_id"], "default");
    }

    #[test]
    fn existing_model_is_not_overwritten() {
        let mut g = json!({
            "nodes": [
                {
                    "id": "a",
                    "type": "action",
                    "config": {
                        "execution": "llm",
                        "model": "claude-opus-4-7",
                        "provider_id": "openrouter"
                    }
                }
            ],
            "edges": []
        });
        apply_agent_type_defaults(&mut g, "writer");
        // Writer default would be claude-haiku-4-5, but user-supplied value wins.
        assert_eq!(g["nodes"][0]["config"]["model"], "claude-opus-4-7");
        assert_eq!(g["nodes"][0]["config"]["provider_id"], "openrouter");
    }

    #[test]
    fn non_llm_nodes_are_left_alone() {
        let mut g = json!({
            "nodes": [
                {"id": "a", "type": "inbound", "config": {"execution": "http"}},
                {"id": "b", "type": "action", "config": {"execution": "script"}},
                {"id": "c", "type": "outbound", "config": {"execution": "doc"}}
            ],
            "edges": []
        });
        let before = g.clone();
        apply_agent_type_defaults(&mut g, "writer");
        assert_eq!(g, before, "non-llm nodes must not be modified");
    }

    #[test]
    fn missing_provider_id_filled_with_default() {
        let mut g = json!({
            "nodes": [
                {
                    "id": "a",
                    "type": "action",
                    "config": {
                        "execution": "llm",
                        "model": "claude-sonnet-4-6"
                    }
                }
            ],
            "edges": []
        });
        apply_agent_type_defaults(&mut g, "reasoning");
        // Model stays as the user-supplied value.
        assert_eq!(g["nodes"][0]["config"]["model"], "claude-sonnet-4-6");
        // provider_id is filled in.
        assert_eq!(g["nodes"][0]["config"]["provider_id"], "default");
    }

    #[test]
    fn null_model_is_treated_as_missing() {
        let mut g = json!({
            "nodes": [
                {
                    "id": "a",
                    "type": "action",
                    "config": {
                        "execution": "llm",
                        "model": null,
                        "provider_id": null
                    }
                }
            ],
            "edges": []
        });
        apply_agent_type_defaults(&mut g, "writer");
        assert_eq!(g["nodes"][0]["config"]["model"], "claude-haiku-4-5");
        assert_eq!(g["nodes"][0]["config"]["provider_id"], "default");
    }

    #[test]
    fn unknown_agent_type_falls_back_to_sonnet() {
        let mut g = json!({
            "nodes": [
                {"id": "a", "type": "action", "config": {"execution": "llm"}}
            ],
            "edges": []
        });
        apply_agent_type_defaults(&mut g, "alien");
        // Match arm `_ => DEFAULT_REASONING_MODEL` covers unexpected values.
        assert_eq!(g["nodes"][0]["config"]["model"], "claude-sonnet-4-6");
    }

    #[test]
    fn malformed_graph_does_not_panic() {
        // No "nodes" key.
        let mut g = json!({});
        apply_agent_type_defaults(&mut g, "writer");
        assert_eq!(g, json!({}));

        // Nodes is not an array.
        let mut g = json!({"nodes": "oops"});
        apply_agent_type_defaults(&mut g, "writer");
        assert_eq!(g, json!({"nodes": "oops"}));

        // Node without a config field.
        let mut g = json!({"nodes": [{"id": "a", "type": "action"}], "edges": []});
        let before = g.clone();
        apply_agent_type_defaults(&mut g, "writer");
        assert_eq!(g, before);

        // Node whose config is not an object.
        let mut g = json!({"nodes": [{"id": "a", "type": "action", "config": "no"}], "edges": []});
        let before = g.clone();
        apply_agent_type_defaults(&mut g, "writer");
        assert_eq!(g, before);
    }
}
