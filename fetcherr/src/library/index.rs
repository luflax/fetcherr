//! What the media server holds, by TMDB id: movies, and series down to their episodes.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use crate::titles::{TitleKey, TitleKind};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LibraryItemKind {
    Movie,
    Series,
    Season,
    Episode,
}

/// One media-server item, as read by the provider.
#[derive(Debug, Clone, PartialEq)]
pub struct LibraryItem {
    pub id: String,
    pub kind: LibraryItemKind,
    /// Movies and series only.
    pub tmdb_id: Option<u32>,
    /// Seasons and episodes only.
    pub series_id: Option<String>,
    /// Seasons: their number. Episodes: their season's number.
    pub season_number: Option<u32>,
    pub episode_number: Option<u32>,
    /// Multi-episode files (`S01E01-E02`).
    pub episode_number_end: Option<u32>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct SeasonEntry {
    pub item_id: Option<String>,
    pub episodes: BTreeMap<u32, String>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct SeriesEntry {
    pub item_id: String,
    pub seasons: BTreeMap<u32, SeasonEntry>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct LibraryIndex {
    pub movies: HashMap<u32, String>,
    pub series: HashMap<u32, SeriesEntry>,
}

/// A difference between two syncs, used to tell clients what became (un)available.
#[derive(Debug, Clone, PartialEq)]
pub struct LibraryChange {
    pub key: TitleKey,
    pub season: Option<u32>,
    pub episode: Option<u32>,
    /// The item id now, `None` when it disappeared.
    pub item_id: Option<String>,
}

impl LibraryIndex {
    pub fn build(items: &[LibraryItem]) -> Self {
        let mut index = Self::default();
        let mut tmdb_by_series_item = HashMap::new();
        for item in items {
            let Some(tmdb_id) = item.tmdb_id else {
                continue;
            };
            match item.kind {
                LibraryItemKind::Movie => {
                    index.movies.insert(tmdb_id, item.id.clone());
                }
                LibraryItemKind::Series => {
                    tmdb_by_series_item.insert(item.id.as_str(), tmdb_id);
                    index.series.entry(tmdb_id).or_default().item_id = item.id.clone();
                }
                _ => {}
            }
        }
        for item in items {
            let Some(series) = item
                .series_id
                .as_deref()
                .and_then(|id| tmdb_by_series_item.get(id))
                .and_then(|tmdb_id| index.series.get_mut(tmdb_id))
            else {
                continue;
            };
            let Some(season_number) = item.season_number else {
                continue;
            };
            let season = series.seasons.entry(season_number).or_default();
            match (item.kind, item.episode_number) {
                (LibraryItemKind::Season, _) => season.item_id = Some(item.id.clone()),
                (LibraryItemKind::Episode, Some(first)) => {
                    let last = item.episode_number_end.unwrap_or(first).max(first);
                    for episode in first..=last {
                        season.episodes.insert(episode, item.id.clone());
                    }
                }
                _ => {}
            }
        }
        index
    }

    pub fn movie(&self, tmdb_id: u32) -> Option<&str> {
        self.movies.get(&tmdb_id).map(String::as_str)
    }

    pub fn series(&self, tmdb_id: u32) -> Option<&SeriesEntry> {
        self.series.get(&tmdb_id)
    }

    /// The item id of a title, if it is in the library.
    pub fn item_id(&self, key: TitleKey) -> Option<&str> {
        match key.kind {
            TitleKind::Movie => self.movie(key.tmdb_id),
            TitleKind::Series => self
                .series(key.tmdb_id)
                .map(|series| series.item_id.as_str()),
        }
    }

    pub fn episodes(&self, tmdb_id: u32, season: u32) -> BTreeSet<u32> {
        self.season(tmdb_id, season)
            .map(|season| season.episodes.keys().copied().collect())
            .unwrap_or_default()
    }

    pub fn season(&self, tmdb_id: u32, season: u32) -> Option<&SeasonEntry> {
        self.series(tmdb_id)?.seasons.get(&season)
    }

    /// Titles and episodes added or removed since `previous`.
    pub fn changes_since(&self, previous: &Self) -> Vec<LibraryChange> {
        let mut changes = Vec::new();
        let movie_ids: BTreeSet<u32> = self
            .movies
            .keys()
            .chain(previous.movies.keys())
            .copied()
            .collect();
        for tmdb_id in movie_ids {
            let now = self.movie(tmdb_id);
            if now != previous.movie(tmdb_id) {
                changes.push(LibraryChange {
                    key: TitleKey::new(TitleKind::Movie, tmdb_id),
                    season: None,
                    episode: None,
                    item_id: now.map(str::to_string),
                });
            }
        }
        let series_ids: BTreeSet<u32> = self
            .series
            .keys()
            .chain(previous.series.keys())
            .copied()
            .collect();
        for tmdb_id in series_ids {
            series_changes(
                tmdb_id,
                previous.series(tmdb_id),
                self.series(tmdb_id),
                &mut changes,
            );
        }
        changes
    }
}

fn series_changes(
    tmdb_id: u32,
    before: Option<&SeriesEntry>,
    after: Option<&SeriesEntry>,
    changes: &mut Vec<LibraryChange>,
) {
    if before == after {
        return;
    }
    let key = TitleKey::new(TitleKind::Series, tmdb_id);
    changes.push(LibraryChange {
        key,
        season: None,
        episode: None,
        item_id: after.map(|series| series.item_id.clone()),
    });
    let episodes_of = |entry: Option<&SeriesEntry>| -> BTreeMap<(u32, u32), String> {
        entry
            .into_iter()
            .flat_map(|series| &series.seasons)
            .flat_map(|(season, entry)| {
                entry
                    .episodes
                    .iter()
                    .map(move |(episode, id)| ((*season, *episode), id.clone()))
            })
            .collect()
    };
    let (old, new) = (episodes_of(before), episodes_of(after));
    let all: BTreeSet<(u32, u32)> = old.keys().chain(new.keys()).copied().collect();
    for (season, episode) in all {
        if old.get(&(season, episode)) != new.get(&(season, episode)) {
            changes.push(LibraryChange {
                key,
                season: Some(season),
                episode: Some(episode),
                item_id: new.get(&(season, episode)).cloned(),
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(id: &str, kind: LibraryItemKind) -> LibraryItem {
        LibraryItem {
            id: id.into(),
            kind,
            tmdb_id: None,
            series_id: None,
            season_number: None,
            episode_number: None,
            episode_number_end: None,
        }
    }

    fn episode(id: &str, season: u32, number: u32, end: Option<u32>) -> LibraryItem {
        LibraryItem {
            series_id: Some("s1".into()),
            season_number: Some(season),
            episode_number: Some(number),
            episode_number_end: end,
            ..item(id, LibraryItemKind::Episode)
        }
    }

    fn library() -> Vec<LibraryItem> {
        vec![
            LibraryItem {
                tmdb_id: Some(603),
                ..item("m1", LibraryItemKind::Movie)
            },
            LibraryItem {
                tmdb_id: Some(1399),
                ..item("s1", LibraryItemKind::Series)
            },
            LibraryItem {
                series_id: Some("s1".into()),
                season_number: Some(1),
                ..item("season1", LibraryItemKind::Season)
            },
            episode("e1", 1, 1, None),
            episode("e23", 1, 2, Some(3)),
            // An episode of a series without a TMDB id is ignored.
            LibraryItem {
                series_id: Some("other".into()),
                ..episode("x", 1, 1, None)
            },
        ]
    }

    #[test]
    fn indexes_movies_series_and_episodes_by_tmdb_id() {
        let index = LibraryIndex::build(&library());
        assert_eq!(index.movie(603), Some("m1"));
        assert_eq!(
            index.item_id(TitleKey::new(TitleKind::Series, 1399)),
            Some("s1")
        );
        assert_eq!(index.episodes(1399, 1), BTreeSet::from([1, 2, 3]));
        assert_eq!(
            index.season(1399, 1).unwrap().item_id.as_deref(),
            Some("season1")
        );
        assert_eq!(index.season(1399, 1).unwrap().episodes[&3], "e23");
    }

    #[test]
    fn reports_added_episodes_and_removed_movies() {
        let before = LibraryIndex::build(&library());
        let mut items = library();
        items.remove(0);
        items.push(episode("e4", 1, 4, None));
        let after = LibraryIndex::build(&items);

        let changes = after.changes_since(&before);
        assert!(changes.contains(&LibraryChange {
            key: TitleKey::new(TitleKind::Movie, 603),
            season: None,
            episode: None,
            item_id: None,
        }));
        assert!(changes.contains(&LibraryChange {
            key: TitleKey::new(TitleKind::Series, 1399),
            season: Some(1),
            episode: Some(4),
            item_id: Some("e4".into()),
        }));
        assert!(after.changes_since(&after).is_empty());
    }
}
