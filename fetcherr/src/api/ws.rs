//! `GET /api/ws/downloads?ids=a,b`: JSON [`FeedEvent`] stream fed by the shared monitor.

use std::collections::HashSet;

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Query, State};
use axum::response::Response;
use serde::Deserialize;
use tokio::sync::watch;

use crate::domain::{Download, DownloadId};
use crate::downloads::feed::{DownloadFeed, FeedEvent};
use crate::downloads::monitor::Snapshot;
use crate::error::AppError;
use crate::state::AppState;

#[derive(Deserialize)]
pub struct FeedParams {
    /// Comma-separated download ids; omit to follow every download.
    ids: Option<String>,
}

pub async fn downloads(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
    Query(params): Query<FeedParams>,
) -> Result<Response, AppError> {
    let only = params.ids.as_deref().map(parse_ids).transpose()?;
    let snapshots = state.monitor.subscribe();
    Ok(ws.on_upgrade(move |socket| async move {
        let fresh = crate::downloads::list(&state.integrations()).await.ok();
        stream(socket, snapshots, DownloadFeed::new(only), fresh).await;
    }))
}

fn parse_ids(raw: &str) -> Result<HashSet<DownloadId>, AppError> {
    raw.split(',')
        .map(str::trim)
        .filter(|id| !id.is_empty())
        .map(|id| DownloadId::try_from(id.to_string()))
        .collect()
}

/// Sends a fresh full snapshot first (the monitor may be idle and stale), then every change.
async fn stream(
    mut socket: WebSocket,
    mut snapshots: watch::Receiver<Snapshot>,
    mut feed: DownloadFeed,
    fresh: Option<Vec<Download>>,
) {
    let first = fresh.unwrap_or_else(|| snapshots.borrow().as_ref().clone());
    snapshots.mark_unchanged();
    if send_events(&mut socket, feed.next_events(&first))
        .await
        .is_err()
    {
        return;
    }
    loop {
        tokio::select! {
            changed = snapshots.changed() => if changed.is_err() { return },
            incoming = socket.recv() => {
                if is_closed(incoming) { return }
                continue;
            }
        }
        let snapshot = snapshots.borrow_and_update().clone();
        if send_events(&mut socket, feed.next_events(&snapshot))
            .await
            .is_err()
        {
            return;
        }
    }
}

async fn send_events(socket: &mut WebSocket, events: Vec<FeedEvent>) -> Result<(), axum::Error> {
    for event in events {
        let json = serde_json::to_string(&event).map_err(axum::Error::new)?;
        socket.send(Message::Text(json.into())).await?;
    }
    Ok(())
}

fn is_closed(incoming: Option<Result<Message, axum::Error>>) -> bool {
    matches!(incoming, None | Some(Err(_)) | Some(Ok(Message::Close(_))))
}
