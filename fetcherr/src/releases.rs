//! Remembers where recent search results can be downloaded from, keyed by [`ReleaseId`].

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use crate::domain::{ReleaseId, ReleaseSource};

const TTL: Duration = Duration::from_secs(60 * 60);

pub struct ReleaseCache {
    ttl: Duration,
    entries: Mutex<HashMap<ReleaseId, (Instant, ReleaseSource)>>,
}

impl Default for ReleaseCache {
    fn default() -> Self {
        Self::with_ttl(TTL)
    }
}

impl ReleaseCache {
    pub fn with_ttl(ttl: Duration) -> Self {
        Self {
            ttl,
            entries: Mutex::new(HashMap::new()),
        }
    }

    pub fn remember(&self, releases: impl IntoIterator<Item = (ReleaseId, ReleaseSource)>) {
        let now = Instant::now();
        let mut entries = self.lock();
        entries.retain(|_, (stored_at, _)| now.duration_since(*stored_at) < self.ttl);
        entries.extend(releases.into_iter().map(|(id, source)| (id, (now, source))));
    }

    pub fn get(&self, id: &ReleaseId) -> Option<ReleaseSource> {
        let entries = self.lock();
        let (stored_at, source) = entries.get(id)?;
        (stored_at.elapsed() < self.ttl).then(|| source.clone())
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<ReleaseId, (Instant, ReleaseSource)>> {
        self.entries
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn forgets_expired_releases() {
        let cache = ReleaseCache::with_ttl(Duration::ZERO);
        let id = ReleaseId::from_guid(1, "guid");
        cache.remember([(id.clone(), ReleaseSource::Magnet("magnet:?".into()))]);
        assert_eq!(cache.get(&id), None);
    }

    #[test]
    fn returns_fresh_releases() {
        let cache = ReleaseCache::default();
        let id = ReleaseId::from_guid(1, "guid");
        let source = ReleaseSource::TorrentUrl("http://prowlarr/1/download".into());
        cache.remember([(id.clone(), source.clone())]);
        assert_eq!(cache.get(&id), Some(source));
    }
}
