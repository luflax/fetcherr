//! Turns monitor snapshots into the WebSocket event stream of one viewer:
//! a full update first, then only changed and removed downloads.

use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::domain::{Download, DownloadId};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum FeedEvent {
    Update { downloads: Vec<Download> },
    Removed { ids: Vec<DownloadId> },
}

pub struct DownloadFeed {
    only: Option<HashSet<DownloadId>>,
    sent: HashMap<DownloadId, Download>,
    started: bool,
}

impl DownloadFeed {
    /// `only`: follow just these downloads; `None` follows all of them.
    pub fn new(only: Option<HashSet<DownloadId>>) -> Self {
        Self {
            only,
            sent: HashMap::new(),
            started: false,
        }
    }

    pub fn next_events(&mut self, snapshot: &[Download]) -> Vec<FeedEvent> {
        let visible: Vec<&Download> = snapshot.iter().filter(|d| self.follows(&d.id)).collect();
        let changed: Vec<Download> = visible
            .iter()
            .filter(|download| self.sent.get(&download.id) != Some(download))
            .map(|download| (*download).clone())
            .collect();
        let visible_ids: HashSet<&DownloadId> = visible.iter().map(|d| &d.id).collect();
        let removed: Vec<DownloadId> = self
            .sent
            .keys()
            .filter(|id| !visible_ids.contains(id))
            .cloned()
            .collect();

        for id in &removed {
            self.sent.remove(id);
        }
        for download in &changed {
            self.sent.insert(download.id.clone(), download.clone());
        }

        let mut events = Vec::new();
        if !changed.is_empty() || !self.started {
            events.push(FeedEvent::Update { downloads: changed });
        }
        if !removed.is_empty() {
            events.push(FeedEvent::Removed { ids: removed });
        }
        self.started = true;
        events
    }

    fn follows(&self, id: &DownloadId) -> bool {
        self.only.as_ref().is_none_or(|only| only.contains(id))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::DownloadState;

    fn download(hash_digit: char, progress: f64) -> Download {
        Download {
            id: DownloadId::try_from(hash_digit.to_string().repeat(40)).unwrap(),
            name: format!("d{hash_digit}"),
            state: DownloadState::Downloading,
            progress,
            size: 100,
            downloaded: 0,
            dl_speed: 0,
            ul_speed: 0,
            eta_secs: None,
            seeds: 0,
            peers: 0,
            save_path: String::new(),
            media_type: None,
        }
    }

    #[test]
    fn first_event_is_full_snapshot_even_when_empty() {
        let mut feed = DownloadFeed::new(None);
        assert_eq!(
            feed.next_events(&[]),
            vec![FeedEvent::Update { downloads: vec![] }]
        );
    }

    #[test]
    fn sends_only_changes_then_removals() {
        let mut feed = DownloadFeed::new(None);
        feed.next_events(&[download('a', 0.1), download('b', 0.1)]);

        let events = feed.next_events(&[download('a', 0.2), download('b', 0.1)]);
        assert_eq!(
            events,
            vec![FeedEvent::Update {
                downloads: vec![download('a', 0.2)]
            }]
        );

        let events = feed.next_events(&[download('a', 0.2)]);
        assert_eq!(
            events,
            vec![FeedEvent::Removed {
                ids: vec![download('b', 0.0).id]
            }]
        );

        assert!(feed.next_events(&[download('a', 0.2)]).is_empty());
    }

    #[test]
    fn follows_only_requested_ids() {
        let wanted = download('a', 0.0).id;
        let mut feed = DownloadFeed::new(Some(HashSet::from([wanted])));
        let events = feed.next_events(&[download('a', 0.5), download('b', 0.5)]);
        assert_eq!(
            events,
            vec![FeedEvent::Update {
                downloads: vec![download('a', 0.5)]
            }]
        );
    }
}
