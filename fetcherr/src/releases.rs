//! Remembers where recent search results can be downloaded from, keyed by [`ReleaseId`].

use std::time::Duration;

use crate::cache::TtlCache;
use crate::domain::{MediaType, ReleaseId, ReleaseSource};

const TTL: Duration = Duration::from_secs(60 * 60);
const MAX_RELEASES: usize = 20_000;

/// What a download needs to know about a search result.
#[derive(Debug, Clone, PartialEq)]
pub struct CachedRelease {
    pub source: ReleaseSource,
    pub media_type: Option<MediaType>,
    /// Release name, parsed again when a request uses this release.
    pub title: String,
}

pub struct ReleaseCache {
    ttl: Duration,
    entries: TtlCache<ReleaseId, CachedRelease>,
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
            entries: TtlCache::new(MAX_RELEASES),
        }
    }

    pub fn remember(&self, releases: impl IntoIterator<Item = (ReleaseId, CachedRelease)>) {
        self.entries.extend(releases, self.ttl);
    }

    pub fn get(&self, id: &ReleaseId) -> Option<CachedRelease> {
        self.entries.get(id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn release(source: ReleaseSource) -> CachedRelease {
        CachedRelease {
            source,
            media_type: Some(MediaType::Movies),
            title: "Dune.2021.1080p".into(),
        }
    }

    #[test]
    fn forgets_expired_releases() {
        let cache = ReleaseCache::with_ttl(Duration::ZERO);
        let id = ReleaseId::from_guid(1, "guid");
        cache.remember([(
            id.clone(),
            release(ReleaseSource::Magnet("magnet:?".into())),
        )]);
        assert_eq!(cache.get(&id), None);
    }

    #[test]
    fn returns_fresh_releases() {
        let cache = ReleaseCache::default();
        let id = ReleaseId::from_guid(1, "guid");
        let release = release(ReleaseSource::TorrentUrl(
            "http://prowlarr/1/download".into(),
        ));
        cache.remember([(id.clone(), release.clone())]);
        assert_eq!(cache.get(&id), Some(release));
    }
}
