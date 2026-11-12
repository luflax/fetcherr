//! Title metadata as Fetcherr uses it, independent of the metadata provider (TMDB).
//! Image fields are provider paths; the catalog turns them into [`crate::titles::Images`].

use crate::titles::{TitleKey, TitleKind};

#[derive(Debug, Clone, PartialEq)]
pub struct TitleSummary {
    pub kind: TitleKind,
    pub tmdb_id: u32,
    pub title: String,
    pub original_title: String,
    pub year: Option<i32>,
    pub poster_path: Option<String>,
    pub backdrop_path: Option<String>,
    pub overview: String,
    pub vote_average: f64,
}

impl TitleSummary {
    pub fn key(&self) -> TitleKey {
        TitleKey::new(self.kind, self.tmdb_id)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct PersonSummary {
    pub tmdb_id: u32,
    pub name: String,
    pub profile_path: Option<String>,
    pub known_for_department: Option<String>,
    pub known_for: Vec<TitleSummary>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum SearchHit {
    Title(TitleSummary),
    Person(PersonSummary),
}

#[derive(Debug, Clone, PartialEq)]
pub struct CastMember {
    pub tmdb_id: u32,
    pub name: String,
    pub character: String,
    pub profile_path: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MovieInfo {
    pub summary: TitleSummary,
    pub tagline: String,
    pub runtime: Option<u32>,
    pub release_date: Option<String>,
    pub genres: Vec<String>,
    pub imdb_id: Option<String>,
    pub vote_count: u32,
    pub cast: Vec<CastMember>,
    pub recommendations: Vec<TitleSummary>,
    pub similar: Vec<TitleSummary>,
    pub alternative_titles: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SeasonSummary {
    pub number: u32,
    pub name: String,
    pub episode_count: u32,
    pub air_date: Option<String>,
    pub poster_path: Option<String>,
    pub overview: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SeriesInfo {
    pub summary: TitleSummary,
    pub tagline: String,
    pub first_air_date: Option<String>,
    pub status: Option<String>,
    pub genres: Vec<String>,
    pub imdb_id: Option<String>,
    pub vote_count: u32,
    pub seasons: Vec<SeasonSummary>,
    pub cast: Vec<CastMember>,
    pub recommendations: Vec<TitleSummary>,
    pub similar: Vec<TitleSummary>,
    pub alternative_titles: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EpisodeInfo {
    pub number: u32,
    pub name: String,
    pub overview: String,
    pub air_date: Option<String>,
    pub still_path: Option<String>,
    pub runtime: Option<u32>,
    pub vote_average: f64,
}

impl EpisodeInfo {
    /// Aired on or before `today` (`YYYY-MM-DD`); episodes without a date have not aired.
    pub fn has_aired(&self, today: &str) -> bool {
        self.air_date
            .as_deref()
            .is_some_and(|date| !date.is_empty() && date <= today)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct SeasonInfo {
    pub number: u32,
    pub name: String,
    pub overview: String,
    pub air_date: Option<String>,
    pub poster_path: Option<String>,
    pub episodes: Vec<EpisodeInfo>,
}

impl SeasonInfo {
    pub fn aired_episodes(&self, today: &str) -> std::collections::BTreeSet<u32> {
        self.episodes
            .iter()
            .filter(|episode| episode.has_aired(today))
            .map(|episode| episode.number)
            .collect()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Credit {
    pub title: TitleSummary,
    /// Character for cast credits, job for crew credits.
    pub role: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PersonInfo {
    pub tmdb_id: u32,
    pub name: String,
    pub biography: String,
    pub birthday: Option<String>,
    pub deathday: Option<String>,
    pub place_of_birth: Option<String>,
    pub profile_path: Option<String>,
    pub known_for_department: Option<String>,
    pub cast: Vec<Credit>,
    pub crew: Vec<Credit>,
}

/// Titles to try when matching release names: the title, the original title and aliases.
pub fn search_titles(summary: &TitleSummary, alternatives: &[String]) -> Vec<String> {
    let mut titles = vec![summary.title.clone()];
    for title in std::iter::once(&summary.original_title).chain(alternatives) {
        if !title.is_empty() && !titles.contains(title) {
            titles.push(title.clone());
        }
    }
    titles
}

/// Scores from outside TMDB (through OMDb).
#[derive(Debug, Clone, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ExternalRatings {
    pub imdb: Option<ImdbRating>,
    /// Tomatometer, percent.
    pub rotten_tomatoes: Option<u32>,
    /// Metascore, 0–100.
    pub metacritic: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ImdbRating {
    /// 0–10.
    pub score: f64,
    pub votes: Option<u64>,
}
