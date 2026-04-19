use serde::Serialize;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Mutex;
use tokio::sync::broadcast;
use uuid::Uuid;

#[derive(Clone, Debug, Serialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum RunEvent {
    RunStepStarted { run_id: Uuid, step_id: String },
    RunStepSucceeded { run_id: Uuid, step_id: String, outputs: Value },
    RunStepFailed { run_id: Uuid, step_id: String, error: String },
    RunCompleted { run_id: Uuid, status: String },
}

#[derive(Default)]
pub struct RunBus {
    channels: Mutex<HashMap<Uuid, broadcast::Sender<RunEvent>>>,
}

impl RunBus {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn publish(&self, run_id: Uuid, ev: RunEvent) {
        if let Some(tx) = self.channels.lock().unwrap().get(&run_id) {
            let _ = tx.send(ev);
        }
    }

    pub fn subscribe(&self, run_id: Uuid) -> broadcast::Receiver<RunEvent> {
        let mut guard = self.channels.lock().unwrap();
        guard
            .entry(run_id)
            .or_insert_with(|| broadcast::channel(256).0)
            .subscribe()
    }
}
