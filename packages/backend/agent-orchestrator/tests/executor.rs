use async_trait::async_trait;
use celerex_orchestrator::execution_type::ExecutionType;
use celerex_orchestrator::executor::{Executor, InMemoryRunStore, RunStore};
use celerex_orchestrator::job::{JobRequest, RunStatus, Step, StepKind};
use celerex_orchestrator::tool_dispatch::{DispatchError, Dispatcher, Handler};
use serde_json::{json, Value};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use uuid::Uuid;

struct EchoHandler;
#[async_trait]
impl Handler for EchoHandler {
    async fn invoke(&self, _c: Value, i: Value) -> Result<Value, DispatchError> {
        Ok(i)
    }
}

#[tokio::test]
async fn executor_walks_linear_dag() {
    let mut d = Dispatcher::new();
    d.register(ExecutionType::Http, Arc::new(EchoHandler));
    let store = Arc::new(InMemoryRunStore::default());
    let exec = Executor::new(Arc::new(d), store.clone());

    let run_id = Uuid::new_v4();
    let req = JobRequest {
        run_id,
        engagement_id: Uuid::new_v4(),
        project_id: None,
        steps: vec![
            Step {
                id: "a".into(),
                depends_on: vec![],
                kind: StepKind::Tool {
                    execution: ExecutionType::Http,
                    config: json!({}),
                    inputs: json!({"v": 1}),
                },
            },
            Step {
                id: "b".into(),
                depends_on: vec!["a".into()],
                kind: StepKind::Tool {
                    execution: ExecutionType::Http,
                    config: json!({}),
                    inputs: json!({"v": 2}),
                },
            },
        ],
    };
    let status = exec.run(req).await.unwrap();
    assert_eq!(status, RunStatus::Succeeded);
    let records = store.records_for(run_id);
    assert_eq!(records.len(), 2);
    assert!(records.iter().all(|r| r.status == RunStatus::Succeeded));
}

struct FlakyHandler {
    calls: Arc<AtomicUsize>,
}
#[async_trait]
impl Handler for FlakyHandler {
    async fn invoke(&self, _c: Value, _i: Value) -> Result<Value, DispatchError> {
        let n = self.calls.fetch_add(1, Ordering::SeqCst);
        if n == 0 {
            Err(DispatchError::Handler("transient".into()))
        } else {
            Ok(json!({"ok": true}))
        }
    }
}

struct CountingOk(Arc<AtomicUsize>);
#[async_trait]
impl Handler for CountingOk {
    async fn invoke(&self, _c: Value, _i: Value) -> Result<Value, DispatchError> {
        self.0.fetch_add(1, Ordering::SeqCst);
        Ok(json!({"a": "out"}))
    }
}

#[tokio::test]
async fn retry_resumes_from_failed_step_without_replaying_upstream() {
    let a_calls = Arc::new(AtomicUsize::new(0));
    let b_calls = Arc::new(AtomicUsize::new(0));

    let mut d = Dispatcher::new();
    d.register(ExecutionType::Http, Arc::new(CountingOk(a_calls.clone())));
    d.register(
        ExecutionType::Script,
        Arc::new(FlakyHandler {
            calls: b_calls.clone(),
        }),
    );
    let store = Arc::new(InMemoryRunStore::default());
    let exec = Executor::new(Arc::new(d), store.clone());

    let run_id = Uuid::new_v4();
    let req = JobRequest {
        run_id,
        engagement_id: Uuid::new_v4(),
        project_id: None,
        steps: vec![
            Step {
                id: "a".into(),
                depends_on: vec![],
                kind: StepKind::Tool {
                    execution: ExecutionType::Http,
                    config: json!({}),
                    inputs: json!({}),
                },
            },
            Step {
                id: "b".into(),
                depends_on: vec!["a".into()],
                kind: StepKind::Tool {
                    execution: ExecutionType::Script,
                    config: json!({}),
                    inputs: json!({}),
                },
            },
        ],
    };

    let s1 = exec.run(req.clone()).await.unwrap();
    assert_eq!(s1, RunStatus::Failed);
    assert_eq!(a_calls.load(Ordering::SeqCst), 1);
    assert_eq!(b_calls.load(Ordering::SeqCst), 1);

    let s2 = exec.retry(req).await.unwrap();
    assert_eq!(s2, RunStatus::Succeeded);
    assert_eq!(a_calls.load(Ordering::SeqCst), 1);
    assert_eq!(b_calls.load(Ordering::SeqCst), 2);
}
