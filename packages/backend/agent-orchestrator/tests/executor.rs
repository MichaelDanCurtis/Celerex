use async_trait::async_trait;
use celerex_orchestrator::execution_type::ExecutionType;
use celerex_orchestrator::executor::{Executor, InMemoryRunStore, RunStore};
use celerex_orchestrator::job::{JobRequest, RunStatus, Step, StepKind};
use celerex_orchestrator::tool_dispatch::{DispatchError, Dispatcher, Handler};
use serde_json::{json, Value};
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
