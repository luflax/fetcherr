//! Live events for clients: request progress and availability changes. Services publish on one
//! broadcast channel; each WebSocket filters what its viewer asked for.

use serde::Serialize;
use tokio::sync::broadcast;
use utoipa::ToSchema;

use crate::requests::{RequestId, RequestSummary};
use crate::titles::Availability;

const CAPACITY: usize = 512;

/// A message of `GET /api/ws/events`, sent as one JSON text frame. `type` tells which.
#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Event {
    /// A request was created or changed state (also sent for each matching request on connect).
    RequestUpdated {
        request: Box<RequestSummary>,
    },
    /// Download progress of a request, at most once per qBittorrent poll.
    RequestProgress {
        request_id: RequestId,
        key: String,
        /// Fraction from 0.0 to 1.0 over all of the request's torrents.
        progress: f64,
        /// Bytes per second.
        dl_speed: u64,
        /// Seconds left, absent when unknown.
        eta_secs: Option<u64>,
    },
    RequestDeleted {
        request_id: RequestId,
        key: String,
    },
    /// A title, season or episode appeared in or disappeared from the media server.
    AvailabilityChanged {
        key: String,
        availability: Availability,
        jellyfin_item_id: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        season: Option<u32>,
        #[serde(skip_serializing_if = "Option::is_none")]
        episode: Option<u32>,
    },
}

impl Event {
    pub fn request_id(&self) -> Option<RequestId> {
        match self {
            Self::RequestUpdated { request } => Some(request.id),
            Self::RequestProgress { request_id, .. } | Self::RequestDeleted { request_id, .. } => {
                Some(*request_id)
            }
            Self::AvailabilityChanged { .. } => None,
        }
    }

    pub fn key(&self) -> &str {
        match self {
            Self::RequestUpdated { request } => &request.key,
            Self::RequestProgress { key, .. }
            | Self::RequestDeleted { key, .. }
            | Self::AvailabilityChanged { key, .. } => key,
        }
    }
}

#[derive(Clone)]
pub struct EventBus(broadcast::Sender<Event>);

impl Default for EventBus {
    fn default() -> Self {
        Self(broadcast::channel(CAPACITY).0)
    }
}

impl EventBus {
    /// Sends to every current subscriber; nobody listening is fine.
    pub fn publish(&self, event: Event) {
        let _ = self.0.send(event);
    }

    pub fn subscribe(&self) -> broadcast::Receiver<Event> {
        self.0.subscribe()
    }
}
