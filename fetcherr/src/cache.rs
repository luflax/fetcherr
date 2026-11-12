//! A small in-memory cache whose entries expire, bounded in size.

use std::collections::HashMap;
use std::hash::Hash;
use std::sync::Mutex;
use std::time::{Duration, Instant};

pub struct TtlCache<K, V> {
    max_entries: usize,
    entries: Mutex<HashMap<K, Entry<V>>>,
}

struct Entry<V> {
    expires_at: Instant,
    value: V,
}

impl<K: Eq + Hash + Clone, V: Clone> TtlCache<K, V> {
    pub fn new(max_entries: usize) -> Self {
        Self {
            max_entries,
            entries: Mutex::new(HashMap::new()),
        }
    }

    pub fn get(&self, key: &K) -> Option<V> {
        let entries = self.lock();
        let entry = entries.get(key)?;
        (entry.expires_at > Instant::now()).then(|| entry.value.clone())
    }

    pub fn insert(&self, key: K, value: V, ttl: Duration) {
        self.extend([(key, value)], ttl);
    }

    /// Inserts entries, dropping expired ones and then the oldest if the cache is full.
    pub fn extend(&self, items: impl IntoIterator<Item = (K, V)>, ttl: Duration) {
        let now = Instant::now();
        let mut entries = self.lock();
        entries.retain(|_, entry| entry.expires_at > now);
        for (key, value) in items {
            entries.insert(
                key,
                Entry {
                    expires_at: now + ttl,
                    value,
                },
            );
        }
        evict_oldest(&mut entries, self.max_entries);
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<K, Entry<V>>> {
        self.entries
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

fn evict_oldest<K: Eq + Hash + Clone, V>(entries: &mut HashMap<K, Entry<V>>, max: usize) {
    if entries.len() <= max {
        return;
    }
    let mut by_age: Vec<(Instant, K)> = entries
        .iter()
        .map(|(key, entry)| (entry.expires_at, key.clone()))
        .collect();
    by_age.sort_by_key(|(expires_at, _)| *expires_at);
    let excess = entries.len() - max;
    for (_, key) in by_age.into_iter().take(excess) {
        entries.remove(&key);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn forgets_expired_entries() {
        let cache = TtlCache::new(10);
        cache.insert("a", 1, Duration::ZERO);
        assert_eq!(cache.get(&"a"), None);
        cache.insert("b", 2, Duration::from_secs(60));
        assert_eq!(cache.get(&"b"), Some(2));
    }

    #[test]
    fn evicts_the_oldest_entries_when_full() {
        let cache = TtlCache::new(2);
        cache.insert("a", 1, Duration::from_secs(10));
        cache.insert("b", 2, Duration::from_secs(20));
        cache.insert("c", 3, Duration::from_secs(30));
        assert_eq!(cache.get(&"a"), None);
        assert_eq!(cache.get(&"b"), Some(2));
        assert_eq!(cache.get(&"c"), Some(3));
    }
}
