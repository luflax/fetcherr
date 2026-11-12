//! What the catalog endpoints return.

use serde::Serialize;
use utoipa::ToSchema;

use crate::domain::Page;
use crate::requests::{RequestState, RequestSummary};
use crate::settings::ListSource;
use crate::titles::{Availability, Card, Images};

/// A discover page as a tab.
#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
pub struct PageTab {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
pub struct DiscoverPageView {
    pub id: String,
    pub name: String,
    pub lists: Vec<DiscoverListView>,
}

/// A list with its first page of cards (fetch more with `GET /api/discover/lists/{id}`).
#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
pub struct DiscoverListView {
    pub id: String,
    pub title: String,
    pub source: ListSource,
    pub cards: Page<Card>,
    /// Why the list is empty when TMDB failed for it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
pub struct PersonCard {
    /// `person:<tmdb_id>`.
    pub key: String,
    pub tmdb_id: u32,
    pub name: String,
    pub profile: Option<Images>,
    pub known_for_department: Option<String>,
    pub known_for: Vec<Card>,
}

/// A search hit: a movie/series card or a person (`result_type` tells which).
#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
#[serde(tag = "result_type", rename_all = "snake_case")]
pub enum SearchResult {
    Title(Card),
    Person(PersonCard),
}

#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
pub struct CastMemberView {
    pub tmdb_id: u32,
    pub name: String,
    pub character: String,
    pub profile: Option<Images>,
}

#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
pub struct RelatedList {
    /// `recommendations` or `similar`.
    pub id: String,
    pub title: String,
    pub cards: Vec<Card>,
}

#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
pub struct MovieView {
    pub card: Card,
    pub tagline: String,
    /// Minutes.
    pub runtime: Option<u32>,
    pub release_date: Option<String>,
    pub genres: Vec<String>,
    pub imdb_id: Option<String>,
    pub vote_count: u32,
    pub cast: Vec<CastMemberView>,
    /// Requests for this movie (usually one).
    pub requests: Vec<RequestSummary>,
    pub related: Vec<RelatedList>,
}

#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
pub struct SeasonCard {
    pub number: u32,
    pub name: String,
    pub overview: String,
    pub air_date: Option<String>,
    pub poster: Option<Images>,
    pub episode_count: u32,
    pub aired_episodes: u32,
    /// Aired episodes present in the media server.
    pub available_episodes: u32,
    pub availability: Availability,
    pub jellyfin_item_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
pub struct SeriesView {
    pub card: Card,
    pub tagline: String,
    pub first_air_date: Option<String>,
    /// TMDB status, e.g. `Returning Series` or `Ended`.
    pub status: Option<String>,
    pub genres: Vec<String>,
    pub imdb_id: Option<String>,
    pub vote_count: u32,
    pub cast: Vec<CastMemberView>,
    pub seasons: Vec<SeasonCard>,
    pub requests: Vec<RequestSummary>,
    pub related: Vec<RelatedList>,
}

#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
pub struct EpisodeView {
    pub number: u32,
    pub name: String,
    pub overview: String,
    pub air_date: Option<String>,
    pub still: Option<Images>,
    /// Minutes.
    pub runtime: Option<u32>,
    pub vote_average: f64,
    pub availability: Availability,
    pub jellyfin_item_id: Option<String>,
    /// State of the request covering this episode, if any.
    pub request_state: Option<RequestState>,
}

#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
pub struct SeasonView {
    /// The series key, `series:<tmdb_id>`.
    pub key: String,
    pub number: u32,
    pub name: String,
    pub overview: String,
    pub air_date: Option<String>,
    pub poster: Option<Images>,
    pub availability: Availability,
    pub jellyfin_item_id: Option<String>,
    pub episodes: Vec<EpisodeView>,
    /// Requests covering this season.
    pub requests: Vec<RequestSummary>,
}

#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
pub struct CreditCard {
    #[serde(flatten)]
    pub card: Card,
    /// Character (cast) or job (crew).
    pub role: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
pub struct PersonView {
    pub tmdb_id: u32,
    pub name: String,
    pub biography: String,
    pub birthday: Option<String>,
    pub deathday: Option<String>,
    pub place_of_birth: Option<String>,
    pub profile: Option<Images>,
    pub known_for_department: Option<String>,
    /// Newest first.
    pub cast: Vec<CreditCard>,
    /// Newest first.
    pub crew: Vec<CreditCard>,
}

#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
pub struct TmdbScore {
    /// 0–10.
    pub score: f64,
    pub votes: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
pub struct ImdbScore {
    /// 0–10.
    pub score: f64,
    pub votes: Option<u64>,
}

/// Scores of a title. IMDb, Rotten Tomatoes and Metacritic need OMDb and an IMDb id.
#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
pub struct RatingsView {
    pub tmdb: TmdbScore,
    pub imdb: Option<ImdbScore>,
    /// Tomatometer, percent.
    pub rotten_tomatoes: Option<u32>,
    /// Metascore, 0–100.
    pub metacritic: Option<u32>,
}
