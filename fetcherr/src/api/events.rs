//! `GET /api/ws/events`: request progress and availability changes as JSON [`Event`]s.

use std::collections::HashSet;

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Query, State};
use axum::response::Response;
use serde::Deserialize;
use tokio::sync::broadcast::{self, error::RecvError};
use utoipa::IntoParams;

use crate::error::{AppError, ErrorBody};
use crate::events::Event;
use crate::requests::RequestId;
use crate::state::AppState;
use crate::titles::TitleKey;

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct EventFilter {
    /// Comma-separated request ids to follow.
    requests: Option<String>,
    /// Comma-separated title keys to follow, e.g. `movie:603,series:1399`.
    titles: Option<String>,
}

/// Which events a viewer gets: those of the listed requests or titles, or all without a filter.
struct Filter {
    requests: Option<HashSet<RequestId>>,
    titles: Option<HashSet<TitleKey>>,
}

impl Filter {
    fn parse(params: &EventFilter) -> Result<Self, AppError> {
        let requests = params
            .requests
            .as_deref()
            .map(|raw| {
                split(raw)
                    .map(|id| {
                        id.parse()
                            .map_err(|_| AppError::Invalid(format!("'{id}' is not a request id")))
                    })
                    .collect::<Result<HashSet<_>, _>>()
            })
            .transpose()?;
        let titles = params
            .titles
            .as_deref()
            .map(|raw| {
                split(raw)
                    .map(str::parse)
                    .collect::<Result<HashSet<TitleKey>, _>>()
            })
            .transpose()?;
        Ok(Self { requests, titles })
    }

    fn matches(&self, event: &Event) -> bool {
        if self.requests.is_none() && self.titles.is_none() {
            return true;
        }
        let by_request = matches!((&self.requests, event.request_id()), (Some(ids), Some(id)) if ids.contains(&id));
        let by_title = self.titles.as_ref().is_some_and(|keys| {
            event
                .key()
                .parse()
                .is_ok_and(|key: TitleKey| keys.contains(&key))
        });
        by_request || by_title
    }
}

fn split(raw: &str) -> impl Iterator<Item = &str> {
    raw.split(',')
        .map(str::trim)
        .filter(|part| !part.is_empty())
}

/// Opens the event stream. On connect (and after falling behind) a `request_updated` is sent for
/// every matching request (every unfinished one without a filter), then events as they happen:
/// `request_updated`, `request_progress`, `request_deleted` and `availability_changed`.
/// Authenticate with `?api_key=` since browsers cannot set WebSocket headers.
#[utoipa::path(
    get, path = "/api/ws/events", tag = "events",
    params(EventFilter),
    responses(
        (status = 101, description = "WebSocket of JSON text frames, one event each", body = Event),
        (status = 401, description = "Missing or wrong API key", body = ErrorBody),
        (status = 422, description = "Invalid filter", body = ErrorBody),
    )
)]
pub async fn events(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
    Query(params): Query<EventFilter>,
) -> Result<Response, AppError> {
    let filter = Filter::parse(&params)?;
    let receiver = state.events.subscribe();
    Ok(ws.on_upgrade(move |socket| stream(socket, state, filter, receiver)))
}

async fn stream(
    mut socket: WebSocket,
    state: AppState,
    filter: Filter,
    mut events: broadcast::Receiver<Event>,
) {
    if send_snapshot(&mut socket, &state, &filter).await.is_err() {
        return;
    }
    loop {
        tokio::select! {
            received = events.recv() => match received {
                Ok(event) if filter.matches(&event) => {
                    if send(&mut socket, &event).await.is_err() { return }
                }
                Ok(_) => {}
                Err(RecvError::Lagged(_)) => {
                    if send_snapshot(&mut socket, &state, &filter).await.is_err() { return }
                }
                Err(RecvError::Closed) => return,
            },
            incoming = socket.recv() => {
                if matches!(incoming, None | Some(Err(_)) | Some(Ok(Message::Close(_)))) { return }
            }
        }
    }
}

async fn send_snapshot(
    socket: &mut WebSocket,
    state: &AppState,
    filter: &Filter,
) -> Result<(), axum::Error> {
    let requests = state
        .requests
        .snapshot(filter.requests.as_ref(), filter.titles.as_ref())
        .await
        .unwrap_or_else(|err| {
            tracing::warn!("event snapshot: {err}");
            Vec::new()
        });
    for request in requests {
        send(
            socket,
            &Event::RequestUpdated {
                request: Box::new(request),
            },
        )
        .await?;
    }
    Ok(())
}

async fn send(socket: &mut WebSocket, event: &Event) -> Result<(), axum::Error> {
    let json = serde_json::to_string(event).map_err(axum::Error::new)?;
    socket.send(Message::Text(json.into())).await
}
