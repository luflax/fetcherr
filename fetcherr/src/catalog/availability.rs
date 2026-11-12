//! Availability rules: the media server first, then requests on their way.

use std::collections::BTreeSet;

use crate::requests::{RequestState, Scope, TitleRequest};
use crate::titles::Availability;

/// `downloading` beats `requested` when several requests cover the same thing.
pub fn from_requests<'a>(
    requests: impl IntoIterator<Item = &'a TitleRequest>,
    covers: impl Fn(&Scope) -> bool,
) -> Option<Availability> {
    let mut found = None;
    for request in requests
        .into_iter()
        .filter(|request| covers(&request.scope))
    {
        match request.state {
            state if state.is_transferring() => return Some(Availability::Downloading),
            RequestState::Searching | RequestState::NotFound => {
                found = Some(Availability::Requested)
            }
            _ => {}
        }
    }
    found
}

pub fn movie(in_library: bool, requests: &[TitleRequest]) -> Availability {
    let requested_and_imported = requests
        .iter()
        .any(|request| request.state == RequestState::Available);
    if in_library || requested_and_imported {
        return Availability::Available;
    }
    from_requests(requests, |_| true).unwrap_or(Availability::Unavailable)
}

/// `complete`: every aired episode is present, when known (else the series counts as partial).
pub fn series(in_library: bool, complete: Option<bool>, requests: &[TitleRequest]) -> Availability {
    if in_library && complete == Some(true) {
        return Availability::Available;
    }
    let on_its_way = from_requests(requests, |_| true);
    match (on_its_way, in_library) {
        (Some(availability), _) => availability,
        (None, true) => Availability::PartiallyAvailable,
        (None, false) => Availability::Unavailable,
    }
}

pub fn season(
    aired: &BTreeSet<u32>,
    present: &BTreeSet<u32>,
    season: u32,
    requests: &[TitleRequest],
) -> Availability {
    let available = aired.intersection(present).count();
    if !aired.is_empty() && available == aired.len() {
        return Availability::Available;
    }
    let on_its_way = from_requests(requests, |scope| scope.season() == Some(season));
    match (on_its_way, available) {
        (Some(availability), _) => availability,
        (None, 0) => Availability::Unavailable,
        (None, _) => Availability::PartiallyAvailable,
    }
}

pub fn episode(
    present: bool,
    season: u32,
    episode: u32,
    requests: &[TitleRequest],
) -> Availability {
    if present {
        return Availability::Available;
    }
    from_requests(requests, |scope| scope.covers_episode(season, episode))
        .unwrap_or(Availability::Unavailable)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(scope: Scope, state: RequestState) -> TitleRequest {
        TitleRequest { scope, state }
    }

    #[test]
    fn movies_prefer_the_library_then_downloads_then_requests() {
        assert_eq!(movie(true, &[]), Availability::Available);
        assert_eq!(
            movie(false, &[request(Scope::Movie, RequestState::Searching)]),
            Availability::Requested
        );
        let both = [
            request(Scope::Movie, RequestState::NotFound),
            request(Scope::Movie, RequestState::Paused),
        ];
        assert_eq!(movie(false, &both), Availability::Downloading);
        assert_eq!(
            movie(false, &[request(Scope::Movie, RequestState::Failed)]),
            Availability::Unavailable
        );
    }

    #[test]
    fn seasons_compare_aired_and_present_episodes() {
        let aired = BTreeSet::from([1, 2, 3]);
        assert_eq!(
            season(&aired, &BTreeSet::from([1, 2, 3, 4]), 1, &[]),
            Availability::Available
        );
        assert_eq!(
            season(&aired, &BTreeSet::from([1]), 1, &[]),
            Availability::PartiallyAvailable
        );
        let downloading = [request(
            Scope::Season { season: 1 },
            RequestState::Downloading,
        )];
        assert_eq!(
            season(&aired, &BTreeSet::from([1]), 1, &downloading),
            Availability::Downloading
        );
        assert_eq!(
            season(&aired, &BTreeSet::new(), 2, &downloading),
            Availability::Unavailable
        );
    }

    #[test]
    fn episodes_follow_the_requests_covering_them() {
        let requests = [request(
            Scope::Episodes {
                season: 1,
                episodes: vec![2],
            },
            RequestState::Searching,
        )];
        assert_eq!(episode(false, 1, 2, &requests), Availability::Requested);
        assert_eq!(episode(false, 1, 3, &requests), Availability::Unavailable);
        assert_eq!(episode(true, 1, 3, &requests), Availability::Available);
    }

    #[test]
    fn series_are_partial_until_known_complete() {
        assert_eq!(series(true, None, &[]), Availability::PartiallyAvailable);
        assert_eq!(series(true, Some(true), &[]), Availability::Available);
        assert_eq!(series(false, None, &[]), Availability::Unavailable);
    }
}
