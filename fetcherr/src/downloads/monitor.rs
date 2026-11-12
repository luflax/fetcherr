//! One poller for all viewers: polls the download client while anyone is watching and
//! publishes the latest snapshot on a `watch` channel (slow viewers just skip to the newest).

use std::sync::Arc;
use std::time::Duration;

use tokio::sync::watch;
use tokio::time::MissedTickBehavior;

use crate::domain::Download;
use crate::error::AppError;
use crate::integrations::SharedIntegrations;
use crate::settings::SettingsStore;

pub type Snapshot = Arc<Vec<Download>>;

#[derive(Clone)]
pub struct DownloadMonitor {
    sender: Arc<watch::Sender<Snapshot>>,
}

impl DownloadMonitor {
    pub fn start(integrations: Arc<SharedIntegrations>, settings: Arc<SettingsStore>) -> Self {
        let (sender, _) = watch::channel(Snapshot::default());
        let sender = Arc::new(sender);
        tokio::spawn(poll_while_watched(sender.clone(), integrations, settings));
        Self { sender }
    }

    pub fn subscribe(&self) -> watch::Receiver<Snapshot> {
        self.sender.subscribe()
    }

    /// The last published snapshot, without starting a poll (fresh only while watched).
    pub fn latest(&self) -> Snapshot {
        self.sender.borrow().clone()
    }
}

async fn poll_while_watched(
    sender: Arc<watch::Sender<Snapshot>>,
    integrations: Arc<SharedIntegrations>,
    settings: Arc<SettingsStore>,
) {
    let mut errors = ErrorLog::default();
    let mut ticker = PollTicker::new(settings.get().download_client.poll_interval_ms);
    loop {
        ticker
            .tick(settings.get().download_client.poll_interval_ms)
            .await;
        if sender.receiver_count() == 0 {
            continue;
        }
        match super::list(&integrations.current()).await {
            Ok(downloads) => {
                errors.clear();
                sender.send_if_modified(|current| replace_if_changed(current, downloads));
            }
            Err(AppError::NotConfigured(_)) => {}
            Err(err) => errors.report(&err),
        }
    }
}

/// Fixed-rate ticks (request time is not added to the period) that follow setting changes.
struct PollTicker {
    period_ms: u64,
    interval: tokio::time::Interval,
}

impl PollTicker {
    fn new(period_ms: u64) -> Self {
        Self {
            period_ms,
            interval: Self::interval(period_ms),
        }
    }

    async fn tick(&mut self, period_ms: u64) {
        if period_ms != self.period_ms {
            *self = Self::new(period_ms);
        }
        self.interval.tick().await;
    }

    fn interval(period_ms: u64) -> tokio::time::Interval {
        let period = Duration::from_millis(period_ms);
        let mut interval = tokio::time::interval_at(tokio::time::Instant::now() + period, period);
        interval.set_missed_tick_behavior(MissedTickBehavior::Delay);
        interval
    }
}

fn replace_if_changed(current: &mut Snapshot, downloads: Vec<Download>) -> bool {
    if **current == downloads {
        return false;
    }
    *current = Arc::new(downloads);
    true
}

/// Logs a polling failure once instead of every tick.
#[derive(Default)]
struct ErrorLog {
    last: Option<String>,
}

impl ErrorLog {
    fn report(&mut self, err: &AppError) {
        let message = err.to_string();
        if self.last.as_ref() != Some(&message) {
            tracing::warn!("download monitor: {message}");
            self.last = Some(message);
        }
    }

    fn clear(&mut self) {
        if self.last.take().is_some() {
            tracing::info!("download monitor recovered");
        }
    }
}
