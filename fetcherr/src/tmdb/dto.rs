//! TMDB API v3 payloads. Source of truth: `docs/upstream/tmdb-v3-openapi.json`.
//! TMDB omits or nulls many fields, so nearly everything is optional.

use serde::Deserialize;

/// Paged list responses (`/trending`, `/movie/popular`, `/discover/*`, `/search/multi`, ...).
#[derive(Debug, Deserialize)]
pub struct Paged<T> {
    #[serde(default)]
    pub page: u32,
    #[serde(default = "Vec::new")]
    pub results: Vec<T>,
    #[serde(default)]
    pub total_pages: u32,
    #[serde(default)]
    pub total_results: u32,
}

/// One entry of a list: a movie, a series or (in mixed lists) a person.
/// `media_type` is only present in mixed lists (trending/all, search/multi, credits).
#[derive(Debug, Deserialize)]
pub struct ListItem {
    pub id: u32,
    pub media_type: Option<String>,
    pub title: Option<String>,
    pub name: Option<String>,
    pub original_title: Option<String>,
    pub original_name: Option<String>,
    pub release_date: Option<String>,
    pub first_air_date: Option<String>,
    pub poster_path: Option<String>,
    pub backdrop_path: Option<String>,
    pub profile_path: Option<String>,
    pub overview: Option<String>,
    pub vote_average: Option<f64>,
    pub known_for_department: Option<String>,
    pub known_for: Option<Vec<ListItem>>,
    /// Cast credits (`combined_credits`).
    pub character: Option<String>,
    /// Crew credits (`combined_credits`).
    pub job: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct Genre {
    pub name: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct Credits {
    #[serde(default)]
    pub cast: Vec<Cast>,
}

#[derive(Debug, Deserialize)]
pub struct Cast {
    pub id: u32,
    pub name: Option<String>,
    pub character: Option<String>,
    pub profile_path: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct AlternativeTitle {
    pub title: Option<String>,
}

/// Movies list them under `titles`, series under `results`.
#[derive(Debug, Deserialize)]
pub struct AlternativeTitles {
    pub titles: Option<Vec<AlternativeTitle>>,
    pub results: Option<Vec<AlternativeTitle>>,
}

#[derive(Debug, Deserialize)]
pub struct ExternalIds {
    pub imdb_id: Option<String>,
}

/// `GET /3/movie/{movie_id}` with `append_to_response`.
#[derive(Debug, Deserialize)]
pub struct MovieDetails {
    pub id: u32,
    pub title: Option<String>,
    pub original_title: Option<String>,
    pub overview: Option<String>,
    pub tagline: Option<String>,
    pub runtime: Option<u32>,
    pub release_date: Option<String>,
    pub poster_path: Option<String>,
    pub backdrop_path: Option<String>,
    pub vote_average: Option<f64>,
    pub vote_count: Option<u32>,
    pub imdb_id: Option<String>,
    #[serde(default)]
    pub genres: Vec<Genre>,
    pub credits: Option<Credits>,
    pub recommendations: Option<Paged<ListItem>>,
    pub similar: Option<Paged<ListItem>>,
    pub alternative_titles: Option<AlternativeTitles>,
    pub external_ids: Option<ExternalIds>,
}

#[derive(Debug, Deserialize)]
pub struct SeasonSummary {
    pub season_number: u32,
    pub name: Option<String>,
    pub episode_count: Option<u32>,
    pub air_date: Option<String>,
    pub poster_path: Option<String>,
    pub overview: Option<String>,
}

/// `GET /3/tv/{series_id}` with `append_to_response`.
#[derive(Debug, Deserialize)]
pub struct SeriesDetails {
    pub id: u32,
    pub name: Option<String>,
    pub original_name: Option<String>,
    pub overview: Option<String>,
    pub tagline: Option<String>,
    pub first_air_date: Option<String>,
    pub status: Option<String>,
    pub poster_path: Option<String>,
    pub backdrop_path: Option<String>,
    pub vote_average: Option<f64>,
    pub vote_count: Option<u32>,
    #[serde(default)]
    pub genres: Vec<Genre>,
    #[serde(default)]
    pub seasons: Vec<SeasonSummary>,
    pub credits: Option<Credits>,
    pub recommendations: Option<Paged<ListItem>>,
    pub similar: Option<Paged<ListItem>>,
    pub alternative_titles: Option<AlternativeTitles>,
    pub external_ids: Option<ExternalIds>,
}

#[derive(Debug, Deserialize)]
pub struct Episode {
    pub episode_number: u32,
    pub name: Option<String>,
    pub overview: Option<String>,
    pub air_date: Option<String>,
    pub still_path: Option<String>,
    pub runtime: Option<u32>,
    pub vote_average: Option<f64>,
}

/// `GET /3/tv/{series_id}/season/{season_number}`.
#[derive(Debug, Deserialize)]
pub struct SeasonDetails {
    pub season_number: u32,
    pub name: Option<String>,
    pub overview: Option<String>,
    pub air_date: Option<String>,
    pub poster_path: Option<String>,
    #[serde(default)]
    pub episodes: Vec<Episode>,
}

#[derive(Debug, Deserialize)]
pub struct CombinedCredits {
    #[serde(default)]
    pub cast: Vec<ListItem>,
    #[serde(default)]
    pub crew: Vec<ListItem>,
}

/// `GET /3/person/{person_id}` with `append_to_response=combined_credits`.
#[derive(Debug, Deserialize)]
pub struct PersonDetails {
    pub id: u32,
    pub name: Option<String>,
    pub biography: Option<String>,
    pub birthday: Option<String>,
    pub deathday: Option<String>,
    pub place_of_birth: Option<String>,
    pub profile_path: Option<String>,
    pub known_for_department: Option<String>,
    pub combined_credits: Option<CombinedCredits>,
}
