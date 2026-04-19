use crate::scheduler::advance::next_after;
use crate::scheduler::store::{Schedule, SchedulesStore};
use chrono::Utc;
use std::sync::Arc;
use std::time::Duration;
use tokio::task::JoinHandle;

pub struct SchedulerLoop<F> {
    pub store: Arc<dyn SchedulesStore>,
    pub tick: Duration,
    pub dispatch: Arc<F>,
}

impl<F> SchedulerLoop<F>
where
    F: Fn(Schedule) -> tokio::task::JoinHandle<()> + Send + Sync + 'static,
{
    pub fn spawn(self) -> JoinHandle<()> {
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(self.tick);
            loop {
                interval.tick().await;
                let now = Utc::now();
                let due = match self.store.due(now).await {
                    Ok(v) => v,
                    Err(e) => {
                        tracing::error!(?e, "scheduler store.due");
                        continue;
                    }
                };
                for s in due {
                    let next = match next_after(&s.cron_expr, &s.timezone, now) {
                        Ok(n) => n,
                        Err(e) => {
                            tracing::error!(?e, schedule=%s.id, "cron advance failed");
                            continue;
                        }
                    };
                    if let Err(e) = self.store.mark_fired(s.id, now, next).await {
                        tracing::error!(?e, schedule=%s.id, "mark_fired");
                        continue;
                    }
                    (self.dispatch)(s);
                }
            }
        })
    }
}
