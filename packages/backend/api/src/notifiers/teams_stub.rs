use super::{NotificationEvent, Notifier};
use async_trait::async_trait;
use tokio::sync::Mutex;

#[derive(Default)]
pub struct StubTeamsNotifier {
    pub sent: Mutex<Vec<NotificationEvent>>,
}

#[async_trait]
impl Notifier for StubTeamsNotifier {
    async fn dispatch(&self, event: &NotificationEvent) -> Result<(), String> {
        tracing::info!(
            target: "celerex::notifier",
            "StubTeamsNotifier would send: {}",
            serde_json::to_string(event).unwrap()
        );
        self.sent.lock().await.push(event.clone());
        Ok(())
    }
}
