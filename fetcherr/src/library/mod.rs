//! Availability from the media server: one sync loop keeps a [`LibraryIndex`] of Jellyfin's
//! library, every few minutes and whenever something asks for a fresh look.

mod index;

use std::sync::Arc;
use std::time::Duration;

use tokio::sync::{Notify, watch};

use crate::error::AppError;
use crate::events::{Event, EventBus};
use crate::integrations::SharedIntegrations;
use crate::settings::SettingsStore;
use crate::titles::{Availability, TitleKind};

pub use index::{LibraryChange, LibraryIndex, LibraryItem, LibraryItemKind};

/// After an import, Jellyfin needs a moment to scan; look again a few times.
const RESYNC_DELAYS: [Duration; 3] = [
    Duration::from_secs(15),
    Duration::from_secs(60),
    Duration::from_secs(180),
];

#[derive(Clone)]
pub struct Library {
    index: Arc<watch::Sender<Arc<LibraryIndex>>>,
    wake: Arc<Notify>,
}

impl Library {
    pub fn start(
        integrations: Arc<SharedIntegrations>,
        settings: Arc<SettingsStore>,
        events: EventBus,
    ) -> Self {
        let (index, _) = watch::channel(Arc::new(LibraryIndex::default()));
        let library = Self {
            index: Arc::new(index),
            wake: Arc::new(Notify::new()),
        };
        tokio::spawn(sync_loop(library.clone(), integrations, settings, events));
        library
    }

    pub fn index(&self) -> Arc<LibraryIndex> {
        self.index.borrow().clone()
    }

    /// Syncs now instead of waiting for the next interval.
    pub fn sync_now(&self) {
        self.wake.notify_one();
    }

    /// Syncs a few times over the next minutes, while the media server scans new files.
    pub fn sync_after_import(&self) {
        let wake = self.wake.clone();
        tokio::spawn(async move {
            let mut waited = Duration::ZERO;
            for delay in RESYNC_DELAYS {
                tokio::time::sleep(delay - waited).await;
                waited = delay;
                wake.notify_one();
            }
        });
    }
}

async fn sync_loop(
    library: Library,
    integrations: Arc<SharedIntegrations>,
    settings: Arc<SettingsStore>,
    events: EventBus,
) {
    let mut loaded = false;
    let mut last_error = None;
    loop {
        match fetch_index(&integrations).await {
            Ok(index) => {
                if last_error.take().is_some() {
                    tracing::info!("library sync recovered");
                }
                let previous = library.index();
                if loaded {
                    for change in index.changes_since(&previous) {
                        events.publish(availability_event(&change));
                    }
                }
                loaded = true;
                library.index.send_replace(Arc::new(index));
            }
            Err(AppError::NotConfigured(_)) => {
                library
                    .index
                    .send_replace(Arc::new(LibraryIndex::default()));
                loaded = false;
            }
            Err(err) => {
                let message = err.to_string();
                if last_error.as_ref() != Some(&message) {
                    tracing::warn!("library sync: {message}");
                    last_error = Some(message);
                }
            }
        }
        let minutes = settings.get().jellyfin.sync_interval_minutes.max(1);
        tokio::select! {
            _ = tokio::time::sleep(Duration::from_secs(u64::from(minutes) * 60)) => {}
            _ = library.wake.notified() => {}
        }
    }
}

async fn fetch_index(integrations: &SharedIntegrations) -> Result<LibraryIndex, AppError> {
    let integrations = integrations.current();
    let items = integrations.jellyfin()?.library_items().await?;
    Ok(LibraryIndex::build(&items))
}

/// Series-level changes say `partially_available`: whether every aired episode is present
/// needs TMDB, so clients refetch the series for the exact state.
fn availability_event(change: &LibraryChange) -> Event {
    let present = change.item_id.is_some();
    let availability = match (present, change.key.kind, change.episode) {
        (false, _, _) => Availability::Unavailable,
        (true, TitleKind::Series, None) => Availability::PartiallyAvailable,
        (true, _, _) => Availability::Available,
    };
    Event::AvailabilityChanged {
        key: change.key.to_string(),
        availability,
        jellyfin_item_id: change.item_id.clone(),
        season: change.season,
        episode: change.episode,
    }
}
