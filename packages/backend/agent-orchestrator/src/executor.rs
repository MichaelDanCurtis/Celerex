use crate::job::{JobRequest, RunStatus, Step, StepKind, StepRecord};
use crate::templating::{self, StepContext, TemplateError};
use crate::tool_dispatch::{DispatchError, Dispatcher};
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

pub trait RunStore: Send + Sync {
    fn record(&self, rec: StepRecord);
    fn records_for(&self, run_id: Uuid) -> Vec<StepRecord>;
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
        let order = topological_order(&req.steps)?;
        let mut ctx = StepContext::new();
        for step_id in order {
            let step = req
                .steps
                .iter()
                .find(|s| s.id == step_id)
                .ok_or_else(|| ExecError::UnknownStep(step_id.clone()))?
                .clone();
            let rec = self.run_step(req.run_id, &step, &ctx).await;
            let failed = rec.status == RunStatus::Failed;
            if let Some(out) = rec.outputs.clone() {
                ctx.set(&step.id, out);
            }
            self.store.record(rec);
            if failed {
                return Ok(RunStatus::Failed);
            }
        }
        Ok(RunStatus::Succeeded)
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
        let order = topological_order(&req.steps)?;
        for step_id in order {
            if succeeded.contains(&step_id) {
                continue;
            }
            let step = req
                .steps
                .iter()
                .find(|s| s.id == step_id)
                .ok_or_else(|| ExecError::UnknownStep(step_id.clone()))?
                .clone();
            let rec = self.run_step(req.run_id, &step, &ctx).await;
            let failed = rec.status == RunStatus::Failed;
            if let Some(out) = rec.outputs.clone() {
                ctx.set(&step.id, out);
            }
            self.store.record(rec);
            if failed {
                return Ok(RunStatus::Failed);
            }
        }
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
            StepKind::Filter { .. } => StepRecord {
                run_id,
                step_id: step.id.clone(),
                status: RunStatus::Succeeded,
                inputs: serde_json::json!({}),
                outputs: Some(serde_json::json!({})),
                error: None,
            },
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
