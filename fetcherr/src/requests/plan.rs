//! Deciding what to grab: which episodes are still missing, and which releases cover them best.
//! Candidates arrive sorted by preference (quality ranking, then seeders).

use std::collections::BTreeSet;

use super::store::{StoredFile, StoredTorrent};
use crate::release_name::Coverage;
use crate::search::TitleRelease;

/// Episodes of `season` already in the library, imported, or in a torrent on its way.
pub fn covered_episodes(
    season: u32,
    in_library: &BTreeSet<u32>,
    imported: &[StoredFile],
    torrents: &[StoredTorrent],
) -> BTreeSet<u32> {
    let imported = imported
        .iter()
        .filter(|file| file.season == Some(season))
        .filter_map(|file| file.episode);
    let downloading = torrents
        .iter()
        .filter(|torrent| torrent.state.is_useful())
        .flat_map(|torrent| match &torrent.coverage {
            Coverage::Episodes {
                season: s,
                episodes,
            } if *s == season => episodes.clone(),
            _ => Vec::new(),
        });
    in_library
        .iter()
        .copied()
        .chain(imported)
        .chain(downloading)
        .collect()
}

fn has_seeders(release: &TitleRelease) -> bool {
    release.release.seeders > 0
}

/// The most preferred movie release that can download.
pub fn best_movie(candidates: &[TitleRelease]) -> Option<&TitleRelease> {
    candidates
        .iter()
        .find(|release| release.coverage == Coverage::Movie && has_seeders(release))
}

/// The most preferred pack holding `season`.
pub fn best_pack(candidates: &[TitleRelease], season: u32) -> Option<&TitleRelease> {
    candidates.iter().find(|release| {
        release.coverage.is_pack() && release.coverage.covers_season(season) && has_seeders(release)
    })
}

/// Single- or multi-episode releases that cover `missing` without anything unwanted, most
/// preferred first, each episode taken once. Returns each release with the episodes it serves.
pub fn pick_episodes(
    candidates: &[TitleRelease],
    season: u32,
    missing: &BTreeSet<u32>,
) -> Vec<(TitleRelease, Vec<u32>)> {
    let mut remaining = missing.clone();
    let mut picked = Vec::new();
    for release in candidates.iter().filter(|release| has_seeders(release)) {
        let Coverage::Episodes {
            season: s,
            episodes,
        } = &release.coverage
        else {
            continue;
        };
        if *s != season || episodes.is_empty() || !episodes.iter().all(|e| remaining.contains(e)) {
            continue;
        }
        for episode in episodes {
            remaining.remove(episode);
        }
        picked.push((release.clone(), episodes.clone()));
    }
    picked
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{DownloadId, Release, ReleaseId};
    use crate::quality::Quality;
    use crate::requests::model::TorrentState;

    fn release(name: &str, coverage: Coverage, seeders: u32) -> TitleRelease {
        TitleRelease {
            release: Release {
                release_id: ReleaseId::from_guid(1, name),
                title: name.into(),
                indexer: "i".into(),
                indexer_id: 1,
                size: 1,
                seeders,
                leechers: 0,
                published_at: String::new(),
                age_hours: 0.0,
                categories: vec![],
                info_url: None,
                quality: Quality::Web1080p,
                media_type: None,
            },
            coverage,
            quality_rank: Some(0),
        }
    }

    fn episodes(season: u32, episodes: &[u32]) -> Coverage {
        Coverage::Episodes {
            season,
            episodes: episodes.to_vec(),
        }
    }

    #[test]
    fn counts_library_imports_and_active_torrents() {
        let torrent = |coverage, state| StoredTorrent {
            request_id: 1,
            id: DownloadId::try_from("b".repeat(40)).unwrap(),
            release_title: String::new(),
            quality: Quality::Unknown,
            size: 0,
            coverage,
            state,
            error: None,
        };
        let file = StoredFile {
            request_id: 1,
            info_hash: String::new(),
            source: String::new(),
            destination: String::new(),
            season: Some(1),
            episode: Some(3),
        };
        let covered = covered_episodes(
            1,
            &BTreeSet::from([1]),
            &[file],
            &[
                torrent(episodes(1, &[4, 5]), TorrentState::Downloading),
                torrent(episodes(1, &[6]), TorrentState::Failed),
                torrent(episodes(2, &[7]), TorrentState::Downloading),
            ],
        );
        assert_eq!(covered, BTreeSet::from([1, 3, 4, 5]));
    }

    #[test]
    fn picks_wanted_episodes_without_extras() {
        let candidates = [
            release("dead", episodes(1, &[2]), 0),
            release("double", episodes(1, &[2, 3]), 9),
            release("too-much", episodes(1, &[4, 5]), 9),
            release("single", episodes(1, &[4]), 9),
            release("again", episodes(1, &[4]), 9),
            release(
                "pack",
                Coverage::SeasonPack {
                    seasons: vec![1],
                    complete: false,
                },
                9,
            ),
        ];
        let picked = pick_episodes(&candidates, 1, &BTreeSet::from([2, 3, 4]));
        let names: Vec<&str> = picked
            .iter()
            .map(|(r, _)| r.release.title.as_str())
            .collect();
        assert_eq!(names, ["double", "single"]);
        assert_eq!(best_pack(&candidates, 1).unwrap().release.title, "pack");
        assert!(best_pack(&candidates, 2).is_none());
    }

    #[test]
    fn best_movie_needs_seeders() {
        let candidates = [
            release("dead", Coverage::Movie, 0),
            release("alive", Coverage::Movie, 1),
        ];
        assert_eq!(best_movie(&candidates).unwrap().release.title, "alive");
    }
}
