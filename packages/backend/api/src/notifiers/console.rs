use super::{NotificationEvent, Notifier};
use async_trait::async_trait;

pub struct ConsoleNotifier;

#[async_trait]
impl Notifier for ConsoleNotifier {
    async fn dispatch(&self, event: &NotificationEvent) -> Result<(), String> {
        tracing::info!(
            target: "celerex::notifier",
            tag = event.tag(),
            payload = %event.payload(),
            "console notification"
        );
        Ok(())
    }
}
