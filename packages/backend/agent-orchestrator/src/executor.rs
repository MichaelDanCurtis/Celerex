use crate::job::{JobRequest, RunStatus, Step, StepKind, StepRecord};
use crate::templating::{self, StepContext, TemplateError};
use crate::tool_dispatch::{DispatchError, Dispatcher};
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum ExecError {
    #[error("dispatch error: {0}")]
    Dispatch(#[from] DispatchError),
    #[error("cycle detected involving step {0}")]
    Cycle(String),
    #[error("unknown step {0}")]
    UnknownStep(String),
}

/// Persistence + live-event sink for a run's per-step results.
///
/// `record` is the only required method (terminal per-step results). The
/// lifecycle hooks `started`/`completed` are optional (default no-op) so
/// existing stores keep compiling; the Postgres-backed store overrides them
/// to publish `RunStepStarted` / `RunCompleted` onto the SSE bus, which the
/// frontend's live monitor already expects.
pub trait RunStore: Send + Sync {
    fn record(&self, rec: StepRecord);
    fn records_for(&self, run_id: Uuid) -> Vec<StepRecord>;
    /// Called immediately before a step is dispatched.
    fn started(&self, _run_id: Uuid, _step_id: &str) {}
    /// Called once when the whole run reaches a terminal status.
    fn completed(&self, _run_id: Uuid, _status: RunStatus) {}
}

#[derive(Default)]
pub struct InMemoryRunStore {
    inner: Mutex<HashMap<Uuid, Vec<StepRecord>>>,
}

impl InMemoryRunStore {
    pub fn new() -> Self {
        Self::default()
    }
}

impl RunStore for InMemoryRunStore {
    fn record(&self, rec: StepRecord) {
        self.inner
            .lock()
            .unwrap()
            .entry(rec.run_id)
            .or_default()
            .push(rec);
    }
    fn records_for(&self, run_id: Uuid) -> Vec<StepRecord> {
        self.inner
            .lock()
            .unwrap()
            .get(&run_id)
            .cloned()
            .unwrap_or_default()
    }
}

pub struct Executor {
    dispatcher: Arc<Dispatcher>,
    store: Arc<dyn RunStore>,
}

impl Executor {
    pub fn new(dispatcher: Arc<Dispatcher>, store: Arc<dyn RunStore>) -> Self {
        Self { dispatcher, store }
    }

    pub async fn run(&self, req: JobRequest) -> Result<RunStatus, ExecError> {
        self.execute(&req, StepContext::new(), &HashSet::new())
            .await
    }

    /// Resume a paused run after a human review decision.
    ///
    /// Unlike `retry`, the caller supplies the rebuilt context directly — the
    /// approval endpoint loads prior `run_steps` from the database (async) and
    /// seeds `ctx` with succeeded upstream outputs PLUS the human decision keyed
    /// by the review step id; `already_done` lists the steps to skip. The review
    /// step is intentionally NOT in `already_done` so `execute` re-processes it,
    /// sees its decision in `ctx`, and opens or closes the gate accordingly.
    pub async fn resume_from(
        &self,
        req: JobRequest,
        ctx: StepContext,
        already_done: HashSet<String>,
    ) -> Result<RunStatus, ExecError> {
        self.execute(&req, ctx, &already_done).await
    }

    pub async fn retry(&self, req: JobRequest) -> Result<RunStatus, ExecError> {
        let prior = self.store.records_for(req.run_id);
        let succeeded: HashSet<String> = prior
            .iter()
            .filter(|r| r.status == RunStatus::Succeeded)
            .map(|r| r.step_id.clone())
            .collect();
        // Seed the templating context with outputs from already-succeeded steps
        // so resumed steps see the same upstream values they would on a fresh run.
        let mut ctx = StepContext::new();
        for rec in &prior {
            if rec.status == RunStatus::Succeeded {
                if let Some(out) = &rec.outputs {
                    ctx.set(&rec.step_id, out.clone());
                }
            }
        }
        self.execute(&req, ctx, &succeeded).await
    }

    /// Core run loop shared by `run` and `retry`.
    ///
    /// Walks the DAG in topological order. Steps in `already_done` are skipped
    /// (their outputs are assumed pre-seeded in `ctx`). A `Filter` node that
    /// evaluates falsey closes a "gate": every step transitively downstream of
    /// it is skipped (not run, not recorded) so it reads as never-fired in the
    /// UI — this is the conditional/branching behaviour an n8n-style canvas needs.
    /// The first hard failure short-circuits the whole run to `Failed`.
    async fn execute(
        &self,
        req: &JobRequest,
        mut ctx: StepContext,
        already_done: &HashSet<String>,
    ) -> Result<RunStatus, ExecError> {
        let order = topological_order(&req.steps)?;
        // Nodes whose downstream must be skipped (closed gates + skipped nodes).
        let mut blocked: HashSet<String> = HashSet::new();
        for step_id in order {
            if already_done.contains(&step_id) {
                continue;
            }
            let step = req
                .steps
                .iter()
                .find(|s| s.id == step_id)
                .ok_or_else(|| ExecError::UnknownStep(step_id.clone()))?
                .clone();

            // Gate: if any upstream dependency was blocked, skip this node and
            // propagate the block to its own downstream.
            if step.depends_on.iter().any(|d| blocked.contains(d)) {
                blocked.insert(step_id.clone());
                continue;
            }

            // Human-in-the-loop gate. A `Review` step with a seeded decision
            // (resume path) resolves the gate; without one (fresh run) it PAUSES
            // the whole run — persist an `AwaitingReview` marker carrying the
            // draft and return without marking the run terminal.
            if let StepKind::Review {
                prompt,
                content,
                auto_approve,
            } = &step.kind
            {
                // Scheduler-triggered runs auto-approve: synthesize the same
                // approved decision a human resume would produce so downstream
                // `{{steps.<id>.outputs.content}}` resolves to the reviewed
                // content and the run continues to completion. Only applies on a
                // fresh hit (no seeded decision) — a seeded decision always wins.
                if *auto_approve && ctx.output(&step.id).is_none() {
                    let approved_content =
                        templating::resolve(content, &ctx).unwrap_or(Value::Null);
                    let decision = serde_json::json!({
                        "approved": true,
                        "content": approved_content,
                    });
                    self.store.started(req.run_id, &step.id);
                    self.store.record(StepRecord {
                        run_id: req.run_id,
                        step_id: step.id.clone(),
                        status: RunStatus::Succeeded,
                        inputs: serde_json::json!({ "prompt": prompt, "auto_approved": true }),
                        outputs: Some(decision.clone()),
                        error: None,
                    });
                    ctx.set(&step.id, decision);
                    continue;
                }
                match ctx.output(&step.id).cloned() {
                    Some(decision) => {
                        let approved = decision
                            .get("approved")
                            .and_then(|v| v.as_bool())
                            .unwrap_or(false);
                        self.store.record(StepRecord {
                            run_id: req.run_id,
                            step_id: step.id.clone(),
                            status: RunStatus::Succeeded,
                            inputs: serde_json::json!({ "prompt": prompt }),
                            outputs: Some(decision),
                            error: None,
                        });
                        // The decision is already in `ctx` under this step id, so
                        // downstream `{{steps.<id>.outputs.content}}` resolves to
                        // the (possibly edited) approved content.
                        if !approved {
                            blocked.insert(step.id.clone());
                        }
                        continue;
                    }
                    None => {
                        let draft = templating::resolve(content, &ctx).unwrap_or(Value::Null);
                        self.store.started(req.run_id, &step.id);
                        self.store.record(StepRecord {
                            run_id: req.run_id,
                            step_id: step.id.clone(),
                            status: RunStatus::AwaitingReview,
                            inputs: serde_json::json!({ "prompt": prompt, "draft": draft }),
                            outputs: None,
                            error: None,
                        });
                        return Ok(RunStatus::AwaitingReview);
                    }
                }
            }

            self.store.started(req.run_id, &step.id);
            let rec = self.run_step(req.run_id, &step, &ctx).await;
            let failed = rec.status == RunStatus::Failed;
            let gate_closed = matches!(step.kind, StepKind::Filter { .. })
                && rec
                    .outputs
                    .as_ref()
                    .and_then(|o| o.get("passed"))
                    .and_then(|v| v.as_bool())
                    == Some(false);
            if let Some(out) = rec.outputs.clone() {
                ctx.set(&step.id, out);
            }
            self.store.record(rec);
            if failed {
                self.store.completed(req.run_id, RunStatus::Failed);
                return Ok(RunStatus::Failed);
            }
            if gate_closed {
                blocked.insert(step.id.clone());
            }
        }
        self.store.completed(req.run_id, RunStatus::Succeeded);
        Ok(RunStatus::Succeeded)
    }

    async fn run_step(&self, run_id: Uuid, step: &Step, ctx: &StepContext) -> StepRecord {
        match &step.kind {
            StepKind::Tool {
                execution,
                config,
                inputs,
            }
            | StepKind::Outbound {
                execution,
                config,
                inputs,
            } => {
                let resolved_config = match templating::resolve(config, ctx) {
                    Ok(v) => v,
                    Err(e) => return template_fail(run_id, step, inputs.clone(), e),
                };
                let resolved_inputs = match templating::resolve(inputs, ctx) {
                    Ok(v) => v,
                    Err(e) => return template_fail(run_id, step, inputs.clone(), e),
                };
                match self
                    .dispatcher
                    .invoke(*execution, resolved_config, resolved_inputs.clone())
                    .await
                {
                    Ok(out) => StepRecord {
                        run_id,
                        step_id: step.id.clone(),
                        status: RunStatus::Succeeded,
                        inputs: resolved_inputs,
                        outputs: Some(out),
                        error: None,
                    },
                    Err(e) => StepRecord {
                        run_id,
                        step_id: step.id.clone(),
                        status: RunStatus::Failed,
                        inputs: resolved_inputs,
                        outputs: None,
                        error: Some(e.to_string()),
                    },
                }
            }
            StepKind::Filter { expression } => match evaluate_filter(expression, ctx) {
                Ok(passed) => StepRecord {
                    run_id,
                    step_id: step.id.clone(),
                    status: RunStatus::Succeeded,
                    inputs: serde_json::json!({ "expression": expression }),
                    outputs: Some(serde_json::json!({ "passed": passed })),
                    error: None,
                },
                Err(e) => StepRecord {
                    run_id,
                    step_id: step.id.clone(),
                    status: RunStatus::Failed,
                    inputs: serde_json::json!({ "expression": expression }),
                    outputs: None,
                    error: Some(format!("filter: {e}")),
                },
            },
            // Review steps are intercepted in `execute` (pause/resolve) and never
            // dispatched here.
            StepKind::Review { .. } => {
                unreachable!("Review steps are handled in execute() before dispatch")
            }
        }
    }
}

fn template_fail(
    run_id: Uuid,
    step: &Step,
    raw_inputs: serde_json::Value,
    err: TemplateError,
) -> StepRecord {
    StepRecord {
        run_id,
        step_id: step.id.clone(),
        status: RunStatus::Failed,
        inputs: raw_inputs,
        outputs: None,
        error: Some(format!("template resolve: {err}")),
    }
}

/// Resolve `{{steps.X.outputs.Y}}` references in a filter expression, then
/// evaluate it to a boolean. Supports bare `true`/`false`, single-value
/// truthiness, and binary comparisons (`==`, `!=`, `>`, `<`, `>=`, `<=`,
/// `contains`). Comparisons coerce to numbers when both sides parse as numbers,
/// otherwise compare as (quote-stripped) strings.
fn evaluate_filter(expression: &str, ctx: &StepContext) -> Result<bool, TemplateError> {
    let resolved = templating::resolve(&Value::String(expression.to_string()), ctx)?;
    Ok(eval_resolved(resolved))
}

fn eval_resolved(v: Value) -> bool {
    if let Value::String(s) = &v {
        if let Some(b) = eval_str_expr(s) {
            return b;
        }
    }
    value_truthy(&v)
}

fn value_truthy(v: &Value) -> bool {
    match v {
        Value::Bool(b) => *b,
        Value::Null => false,
        Value::Number(n) => n.as_f64().map(|f| f != 0.0).unwrap_or(true),
        Value::String(s) => !s.trim().is_empty() && !s.trim().eq_ignore_ascii_case("false"),
        Value::Array(a) => !a.is_empty(),
        Value::Object(o) => !o.is_empty(),
    }
}

fn eval_str_expr(s: &str) -> Option<bool> {
    let s = s.trim();
    if s.eq_ignore_ascii_case("true") {
        return Some(true);
    }
    if s.eq_ignore_ascii_case("false") {
        return Some(false);
    }
    // Order matters: check 2-char operators before their 1-char prefixes.
    for op in [">=", "<=", "==", "!="] {
        if let Some(idx) = s.find(op) {
            return Some(compare(&s[..idx], &s[idx + op.len()..], op));
        }
    }
    for op in [">", "<"] {
        if let Some(idx) = s.find(op) {
            return Some(compare(&s[..idx], &s[idx + op.len()..], op));
        }
    }
    if let Some(idx) = s.find(" contains ") {
        let lhs = strip_quotes(s[..idx].trim());
        let rhs = strip_quotes(s[idx + " contains ".len()..].trim());
        return Some(lhs.contains(&rhs));
    }
    None
}

fn strip_quotes(s: &str) -> String {
    s.trim().trim_matches('"').trim_matches('\'').to_string()
}

fn compare(lhs: &str, rhs: &str, op: &str) -> bool {
    let l = strip_quotes(lhs);
    let r = strip_quotes(rhs);
    if let (Ok(a), Ok(b)) = (l.parse::<f64>(), r.parse::<f64>()) {
        match op {
            ">=" => a >= b,
            "<=" => a <= b,
            ">" => a > b,
            "<" => a < b,
            "==" => (a - b).abs() < f64::EPSILON,
            "!=" => (a - b).abs() >= f64::EPSILON,
            _ => false,
        }
    } else {
        match op {
            "==" => l == r,
            "!=" => l != r,
            ">=" => l >= r,
            "<=" => l <= r,
            ">" => l > r,
            "<" => l < r,
            _ => false,
        }
    }
}

fn topological_order(steps: &[Step]) -> Result<Vec<String>, ExecError> {
    let mut order = Vec::new();
    let mut visited: HashSet<String> = HashSet::new();
    let mut stack: HashSet<String> = HashSet::new();
    fn visit(
        id: &str,
        steps: &[Step],
        visited: &mut HashSet<String>,
        stack: &mut HashSet<String>,
        order: &mut Vec<String>,
    ) -> Result<(), ExecError> {
        if visited.contains(id) {
            return Ok(());
        }
        if !stack.insert(id.to_string()) {
            return Err(ExecError::Cycle(id.into()));
        }
        let step = steps
            .iter()
            .find(|s| s.id == id)
            .ok_or_else(|| ExecError::UnknownStep(id.to_string()))?;
        for d in &step.depends_on {
            visit(d, steps, visited, stack, order)?;
        }
        stack.remove(id);
        visited.insert(id.to_string());
        order.push(id.to_string());
        Ok(())
    }
    for s in steps {
        visit(&s.id, steps, &mut visited, &mut stack, &mut order)?;
    }
    Ok(order)
}

#[cfg(test)]
mod filter_tests {
    use super::*;
    use serde_json::json;

    fn ctx_with(step: &str, out: Value) -> StepContext {
        StepContext::new().with_output(step, out)
    }

    #[test]
    fn bare_bools() {
        let ctx = StepContext::new();
        assert!(evaluate_filter("true", &ctx).unwrap());
        assert!(!evaluate_filter("false", &ctx).unwrap());
    }

    #[test]
    fn numeric_comparison_from_upstream() {
        let ctx = ctx_with("metrics", json!({ "count": 42 }));
        assert!(evaluate_filter("{{steps.metrics.outputs.count}} > 5", &ctx).unwrap());
        assert!(!evaluate_filter("{{steps.metrics.outputs.count}} < 5", &ctx).unwrap());
        assert!(evaluate_filter("{{steps.metrics.outputs.count}} >= 42", &ctx).unwrap());
    }

    #[test]
    fn string_equality_and_contains() {
        let ctx = ctx_with("s", json!({ "status": "success", "msg": "all good here" }));
        assert!(evaluate_filter("{{steps.s.outputs.status}} == success", &ctx).unwrap());
        assert!(!evaluate_filter("{{steps.s.outputs.status}} == fail", &ctx).unwrap());
        assert!(evaluate_filter("{{steps.s.outputs.msg}} contains good", &ctx).unwrap());
    }

    #[test]
    fn whole_template_bool_value() {
        let ctx = ctx_with("g", json!({ "ok": true, "bad": false }));
        assert!(evaluate_filter("{{steps.g.outputs.ok}}", &ctx).unwrap());
        assert!(!evaluate_filter("{{steps.g.outputs.bad}}", &ctx).unwrap());
    }

    #[test]
    fn truthiness_of_strings_and_numbers() {
        let ctx = ctx_with("g", json!({ "name": "x", "empty": "", "zero": 0, "n": 3 }));
        assert!(evaluate_filter("{{steps.g.outputs.name}}", &ctx).unwrap());
        assert!(!evaluate_filter("{{steps.g.outputs.empty}}", &ctx).unwrap());
        assert!(!evaluate_filter("{{steps.g.outputs.zero}}", &ctx).unwrap());
        assert!(evaluate_filter("{{steps.g.outputs.n}}", &ctx).unwrap());
    }
}
