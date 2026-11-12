//! TMDB API v3 client with a response cache. DTOs stay private; callers get [`crate::metadata`].

mod dto;

use std::sync::Arc;
use std::time::Duration;

use reqwest::{StatusCode, Url};
use serde::de::DeserializeOwned;

use crate::cache::TtlCache;
use crate::domain::Page;
use crate::metadata::{
    CastMember, Credit, EpisodeInfo, MovieInfo, PersonInfo, PersonSummary, SearchHit, SeasonInfo,
    SeasonSummary, SeriesInfo, TitleSummary,
};
use crate::settings::layout::DiscoverFilters;
use crate::settings::{ListSource, TmdbSettings};
use crate::titles::{TitleKind, year_of};
use crate::upstream::{UpstreamError, ensure_success, parse_base_url};

const SERVICE: &str = "TMDB";
const DEFAULT_URL: &str = "https://api.themoviedb.org";
/// TMDB refuses pages above 500.
pub const MAX_PAGE: u32 = 500;
const LIST_TTL: Duration = Duration::from_secs(60 * 60);
const DETAILS_TTL: Duration = Duration::from_secs(12 * 60 * 60);
const SEARCH_TTL: Duration = Duration::from_secs(10 * 60);
const MAX_CACHED_RESPONSES: usize = 2_000;
const MAX_CAST: usize = 20;

pub struct TmdbClient {
    http: reqwest::Client,
    base: Url,
    auth: Auth,
    language: String,
    region: String,
    cache: TtlCache<String, Arc<Vec<u8>>>,
}

enum Auth {
    /// v3 API key, sent as the `api_key` query parameter.
    ApiKey(String),
    /// v4 read access token (a JWT), sent as a bearer token.
    Bearer(String),
}

impl Auth {
    fn new(secret: &str) -> Self {
        let secret = secret.trim().to_string();
        if secret.starts_with("eyJ") && secret.contains('.') {
            Self::Bearer(secret)
        } else {
            Self::ApiKey(secret)
        }
    }
}

impl TmdbClient {
    pub fn new(http: reqwest::Client, settings: &TmdbSettings) -> Result<Self, UpstreamError> {
        let url = if settings.url.trim().is_empty() {
            DEFAULT_URL
        } else {
            &settings.url
        };
        Ok(Self {
            http,
            base: parse_base_url(SERVICE, url)?,
            auth: Auth::new(&settings.api_key),
            language: settings.language.clone(),
            region: settings.region.clone(),
            cache: TtlCache::new(MAX_CACHED_RESPONSES),
        })
    }

    /// Checks the credentials with an uncached request.
    pub async fn check(&self) -> Result<(), UpstreamError> {
        let _: serde_json::Value = self.fetch("3/configuration", &[], Duration::ZERO).await?;
        Ok(())
    }

    pub async fn list(
        &self,
        source: ListSource,
        filters: &DiscoverFilters,
        page: u32,
    ) -> Result<Page<TitleSummary>, UpstreamError> {
        let mut params = vec![("page", page.clamp(1, MAX_PAGE).to_string())];
        let path = match source {
            ListSource::Trending { media, window } => {
                let media = media.map_or("all", tmdb_media);
                format!("3/trending/{media}/{}", window.id())
            }
            ListSource::Popular { media } => format!("3/{}/popular", tmdb_media(media)),
            ListSource::TopRated { media } => format!("3/{}/top_rated", tmdb_media(media)),
            ListSource::NowPlaying => "3/movie/now_playing".into(),
            ListSource::Upcoming => "3/movie/upcoming".into(),
            ListSource::OnTheAir => "3/tv/on_the_air".into(),
            ListSource::AiringToday => "3/tv/airing_today".into(),
            ListSource::Discover { media } => {
                params.extend(discover_params(media, filters));
                format!("3/discover/{}", tmdb_media(media))
            }
        };
        if !self.region.is_empty() && source.media() == Some(TitleKind::Movie) {
            params.push(("region", self.region.clone()));
        }
        let paged: dto::Paged<dto::ListItem> = self.fetch(&path, &params, LIST_TTL).await?;
        let default_kind = source.media();
        Ok(to_page(paged, |item| to_summary(item, default_kind)))
    }

    pub async fn search(&self, query: &str, page: u32) -> Result<Page<SearchHit>, UpstreamError> {
        let params = [
            ("query", query.trim().to_string()),
            ("page", page.clamp(1, MAX_PAGE).to_string()),
            ("include_adult", "false".to_string()),
        ];
        let paged: dto::Paged<dto::ListItem> =
            self.fetch("3/search/multi", &params, SEARCH_TTL).await?;
        Ok(to_page(paged, to_search_hit))
    }

    pub async fn movie(&self, tmdb_id: u32) -> Result<MovieInfo, UpstreamError> {
        let params = [(
            "append_to_response",
            "credits,recommendations,similar,alternative_titles,external_ids".to_string(),
        )];
        let movie: dto::MovieDetails = self
            .fetch(&format!("3/movie/{tmdb_id}"), &params, DETAILS_TTL)
            .await?;
        Ok(to_movie(movie))
    }

    pub async fn series(&self, tmdb_id: u32) -> Result<SeriesInfo, UpstreamError> {
        let params = [(
            "append_to_response",
            "credits,recommendations,similar,alternative_titles,external_ids".to_string(),
        )];
        let series: dto::SeriesDetails = self
            .fetch(&format!("3/tv/{tmdb_id}"), &params, DETAILS_TTL)
            .await?;
        Ok(to_series(series))
    }

    pub async fn season(&self, tmdb_id: u32, season: u32) -> Result<SeasonInfo, UpstreamError> {
        let season: dto::SeasonDetails = self
            .fetch(&format!("3/tv/{tmdb_id}/season/{season}"), &[], DETAILS_TTL)
            .await?;
        Ok(to_season(season))
    }

    pub async fn person(&self, tmdb_id: u32) -> Result<PersonInfo, UpstreamError> {
        let params = [("append_to_response", "combined_credits".to_string())];
        let person: dto::PersonDetails = self
            .fetch(&format!("3/person/{tmdb_id}"), &params, DETAILS_TTL)
            .await?;
        Ok(to_person(person))
    }

    /// GETs a path, answering from the cache while fresh.
    async fn fetch<T: DeserializeOwned>(
        &self,
        path: &str,
        params: &[(&str, String)],
        ttl: Duration,
    ) -> Result<T, UpstreamError> {
        let mut url = self
            .base
            .join(path)
            .map_err(|err| UpstreamError::new(SERVICE, err))?;
        url.query_pairs_mut()
            .extend_pairs(params.iter().map(|(name, value)| (*name, value.as_str())))
            .append_pair("language", &self.language);
        let cache_key = url.to_string();

        let body = match self.cache.get(&cache_key) {
            Some(body) => body,
            None => {
                let body = Arc::new(self.download(url).await?);
                if !ttl.is_zero() {
                    self.cache.insert(cache_key, body.clone(), ttl);
                }
                body
            }
        };
        serde_json::from_slice(&body)
            .map_err(|err| UpstreamError::new(SERVICE, format!("unexpected response: {err}")))
    }

    async fn download(&self, mut url: Url) -> Result<Vec<u8>, UpstreamError> {
        let request = match &self.auth {
            Auth::ApiKey(key) => {
                url.query_pairs_mut().append_pair("api_key", key);
                self.http.get(url)
            }
            Auth::Bearer(token) => self.http.get(url).bearer_auth(token),
        };
        let response = request.send().await.map_err(|err| {
            // reqwest errors can contain the URL, and with it the API key.
            UpstreamError::new(SERVICE, format!("request failed: {}", err.without_url()))
        })?;
        match response.status() {
            StatusCode::UNAUTHORIZED => {
                return Err(UpstreamError::new(SERVICE, "API key rejected"));
            }
            StatusCode::NOT_FOUND => {
                return Err(UpstreamError::not_found(SERVICE, "title not found"));
            }
            _ => {}
        }
        let response = ensure_success(SERVICE, response).await?;
        let bytes = response.bytes().await.map_err(|err| {
            UpstreamError::new(SERVICE, format!("request failed: {}", err.without_url()))
        })?;
        Ok(bytes.to_vec())
    }
}

fn tmdb_media(kind: TitleKind) -> &'static str {
    match kind {
        TitleKind::Movie => "movie",
        TitleKind::Series => "tv",
    }
}

fn discover_params(media: TitleKind, filters: &DiscoverFilters) -> Vec<(&'static str, String)> {
    let (date_from, date_to) = match media {
        TitleKind::Movie => ("primary_release_date.gte", "primary_release_date.lte"),
        TitleKind::Series => ("first_air_date.gte", "first_air_date.lte"),
    };
    let pipe = |ids: &[u32]| ids.iter().map(u32::to_string).collect::<Vec<_>>().join("|");
    let mut params = Vec::new();
    if !filters.genres.is_empty() {
        params.push(("with_genres", pipe(&filters.genres)));
    }
    if let Some(year) = filters.year_from {
        params.push((date_from, format!("{year:04}-01-01")));
    }
    if let Some(year) = filters.year_to {
        params.push((date_to, format!("{year:04}-12-31")));
    }
    if let Some(vote) = filters.min_vote {
        params.push(("vote_average.gte", vote.to_string()));
    }
    if let Some(votes) = filters.min_votes {
        params.push(("vote_count.gte", votes.to_string()));
    }
    params.push((
        "sort_by",
        filters
            .sort_by
            .clone()
            .unwrap_or_else(|| "popularity.desc".into()),
    ));
    if let Some(language) = &filters.original_language {
        params.push(("with_original_language", language.clone()));
    }
    if !filters.watch_providers.is_empty() {
        params.push(("with_watch_providers", pipe(&filters.watch_providers)));
    }
    if let Some(region) = &filters.watch_region {
        params.push(("watch_region", region.clone()));
    }
    params
}

fn to_page<T, U>(paged: dto::Paged<T>, map: impl FnMut(T) -> Option<U>) -> Page<U> {
    Page {
        page: paged.page.max(1),
        total_pages: paged.total_pages.min(MAX_PAGE),
        total_results: paged.total_results,
        items: paged.results.into_iter().filter_map(map).collect(),
    }
}

/// A movie or series list entry; people and unknown media types are dropped.
fn to_summary(item: dto::ListItem, default_kind: Option<TitleKind>) -> Option<TitleSummary> {
    let kind = match item.media_type.as_deref() {
        Some("movie") => TitleKind::Movie,
        Some("tv") => TitleKind::Series,
        Some(_) => return None,
        None => default_kind?,
    };
    let (title, original_title, date) = match kind {
        TitleKind::Movie => (item.title, item.original_title, item.release_date),
        TitleKind::Series => (item.name, item.original_name, item.first_air_date),
    };
    let title = title.unwrap_or_default();
    Some(TitleSummary {
        kind,
        tmdb_id: item.id,
        original_title: original_title.unwrap_or_else(|| title.clone()),
        title,
        year: year_of(date.as_deref()),
        poster_path: item.poster_path,
        backdrop_path: item.backdrop_path,
        overview: item.overview.unwrap_or_default(),
        vote_average: item.vote_average.unwrap_or_default(),
    })
}

fn to_search_hit(item: dto::ListItem) -> Option<SearchHit> {
    if item.media_type.as_deref() == Some("person") {
        return Some(SearchHit::Person(PersonSummary {
            tmdb_id: item.id,
            name: item.name.unwrap_or_default(),
            profile_path: item.profile_path,
            known_for_department: item.known_for_department,
            known_for: item
                .known_for
                .unwrap_or_default()
                .into_iter()
                .filter_map(|title| to_summary(title, None))
                .collect(),
        }));
    }
    to_summary(item, None).map(SearchHit::Title)
}

fn list_of(paged: Option<dto::Paged<dto::ListItem>>, kind: TitleKind) -> Vec<TitleSummary> {
    paged
        .map(|paged| paged.results)
        .unwrap_or_default()
        .into_iter()
        .filter_map(|item| to_summary(item, Some(kind)))
        .collect()
}

fn to_cast(credits: Option<dto::Credits>) -> Vec<CastMember> {
    credits
        .map(|credits| credits.cast)
        .unwrap_or_default()
        .into_iter()
        .take(MAX_CAST)
        .map(|cast| CastMember {
            tmdb_id: cast.id,
            name: cast.name.unwrap_or_default(),
            character: cast.character.unwrap_or_default(),
            profile_path: cast.profile_path,
        })
        .collect()
}

fn alternative_titles(titles: Option<dto::AlternativeTitles>) -> Vec<String> {
    titles
        .map(|titles| titles.titles.or(titles.results).unwrap_or_default())
        .unwrap_or_default()
        .into_iter()
        .filter_map(|title| title.title)
        .collect()
}

fn genre_names(genres: Vec<dto::Genre>) -> Vec<String> {
    genres.into_iter().filter_map(|genre| genre.name).collect()
}

fn to_movie(movie: dto::MovieDetails) -> MovieInfo {
    let title = movie.title.unwrap_or_default();
    MovieInfo {
        summary: TitleSummary {
            kind: TitleKind::Movie,
            tmdb_id: movie.id,
            original_title: movie.original_title.unwrap_or_else(|| title.clone()),
            title,
            year: year_of(movie.release_date.as_deref()),
            poster_path: movie.poster_path,
            backdrop_path: movie.backdrop_path,
            overview: movie.overview.unwrap_or_default(),
            vote_average: movie.vote_average.unwrap_or_default(),
        },
        tagline: movie.tagline.unwrap_or_default(),
        runtime: movie.runtime.filter(|minutes| *minutes > 0),
        release_date: movie.release_date.filter(|date| !date.is_empty()),
        genres: genre_names(movie.genres),
        imdb_id: movie
            .imdb_id
            .or(movie.external_ids.and_then(|ids| ids.imdb_id))
            .filter(|id| !id.is_empty()),
        vote_count: movie.vote_count.unwrap_or_default(),
        cast: to_cast(movie.credits),
        recommendations: list_of(movie.recommendations, TitleKind::Movie),
        similar: list_of(movie.similar, TitleKind::Movie),
        alternative_titles: alternative_titles(movie.alternative_titles),
    }
}

fn to_series(series: dto::SeriesDetails) -> SeriesInfo {
    let title = series.name.unwrap_or_default();
    SeriesInfo {
        summary: TitleSummary {
            kind: TitleKind::Series,
            tmdb_id: series.id,
            original_title: series.original_name.unwrap_or_else(|| title.clone()),
            title,
            year: year_of(series.first_air_date.as_deref()),
            poster_path: series.poster_path,
            backdrop_path: series.backdrop_path,
            overview: series.overview.unwrap_or_default(),
            vote_average: series.vote_average.unwrap_or_default(),
        },
        tagline: series.tagline.unwrap_or_default(),
        first_air_date: series.first_air_date.filter(|date| !date.is_empty()),
        status: series.status,
        genres: genre_names(series.genres),
        imdb_id: series
            .external_ids
            .and_then(|ids| ids.imdb_id)
            .filter(|id| !id.is_empty()),
        vote_count: series.vote_count.unwrap_or_default(),
        seasons: series
            .seasons
            .into_iter()
            .map(|season| SeasonSummary {
                number: season.season_number,
                name: season.name.unwrap_or_default(),
                episode_count: season.episode_count.unwrap_or_default(),
                air_date: season.air_date.filter(|date| !date.is_empty()),
                poster_path: season.poster_path,
                overview: season.overview.unwrap_or_default(),
            })
            .collect(),
        cast: to_cast(series.credits),
        recommendations: list_of(series.recommendations, TitleKind::Series),
        similar: list_of(series.similar, TitleKind::Series),
        alternative_titles: alternative_titles(series.alternative_titles),
    }
}

fn to_season(season: dto::SeasonDetails) -> SeasonInfo {
    SeasonInfo {
        number: season.season_number,
        name: season.name.unwrap_or_default(),
        overview: season.overview.unwrap_or_default(),
        air_date: season.air_date.filter(|date| !date.is_empty()),
        poster_path: season.poster_path,
        episodes: season
            .episodes
            .into_iter()
            .map(|episode| EpisodeInfo {
                number: episode.episode_number,
                name: episode.name.unwrap_or_default(),
                overview: episode.overview.unwrap_or_default(),
                air_date: episode.air_date.filter(|date| !date.is_empty()),
                still_path: episode.still_path,
                runtime: episode.runtime,
                vote_average: episode.vote_average.unwrap_or_default(),
            })
            .collect(),
    }
}

fn to_person(person: dto::PersonDetails) -> PersonInfo {
    let credits = person.combined_credits.unwrap_or(dto::CombinedCredits {
        cast: Vec::new(),
        crew: Vec::new(),
    });
    let to_credits = |items: Vec<dto::ListItem>, cast: bool| -> Vec<Credit> {
        items
            .into_iter()
            .filter_map(|item| {
                let role = if cast {
                    item.character.clone()
                } else {
                    item.job.clone()
                };
                to_summary(item, None).map(|title| Credit {
                    title,
                    role: role.unwrap_or_default(),
                })
            })
            .collect()
    };
    PersonInfo {
        tmdb_id: person.id,
        name: person.name.unwrap_or_default(),
        biography: person.biography.unwrap_or_default(),
        birthday: person.birthday,
        deathday: person.deathday,
        place_of_birth: person.place_of_birth,
        profile_path: person.profile_path,
        known_for_department: person.known_for_department,
        cast: to_credits(credits.cast, true),
        crew: to_credits(credits.crew, false),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{header, method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn fixture(name: &str) -> serde_json::Value {
        let path = format!("{}/tests/fixtures/{name}", env!("CARGO_MANIFEST_DIR"));
        serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
    }

    fn client(server: &MockServer, api_key: &str) -> TmdbClient {
        let settings = TmdbSettings {
            url: server.uri(),
            api_key: api_key.into(),
            ..Default::default()
        };
        TmdbClient::new(crate::integrations::http_client(), &settings).unwrap()
    }

    #[tokio::test]
    async fn lists_trending_titles_without_people_and_caches_them() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/3/trending/all/week"))
            .and(query_param("api_key", "key"))
            .and(query_param("page", "2"))
            .and(query_param("language", "en-US"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(fixture("tmdb-trending-all.json")),
            )
            .expect(1)
            .mount(&server)
            .await;
        let client = client(&server, "key");
        let source = ListSource::Trending {
            media: None,
            window: crate::settings::layout::TimeWindow::Week,
        };

        let page = client
            .list(source, &DiscoverFilters::default(), 2)
            .await
            .unwrap();
        let again = client
            .list(source, &DiscoverFilters::default(), 2)
            .await
            .unwrap();

        assert_eq!(page, again);
        assert!(!page.items.is_empty());
        assert_eq!(page.items[0].title, "Scream VI");
        assert_eq!(page.items[0].kind, TitleKind::Movie);
        assert_eq!(page.items[0].year, Some(2023));
    }

    #[tokio::test]
    async fn sends_v4_tokens_as_bearer_and_maps_discover_filters() {
        let server = MockServer::start().await;
        Mock::given(path("/3/discover/tv"))
            .and(header("authorization", "Bearer eyJhbGciOi.payload.sig"))
            .and(query_param("with_genres", "16|35"))
            .and(query_param("first_air_date.gte", "2010-01-01"))
            .and(query_param("sort_by", "popularity.desc"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(fixture("tmdb-discover-tv.json")),
            )
            .expect(1)
            .mount(&server)
            .await;
        let filters = DiscoverFilters {
            genres: vec![16, 35],
            year_from: Some(2010),
            ..Default::default()
        };
        let page = client(&server, "eyJhbGciOi.payload.sig")
            .list(
                ListSource::Discover {
                    media: TitleKind::Series,
                },
                &filters,
                1,
            )
            .await
            .unwrap();
        assert!(
            page.items
                .iter()
                .all(|title| title.kind == TitleKind::Series)
        );
    }

    #[tokio::test]
    async fn reads_movie_details_with_appended_parts() {
        let server = MockServer::start().await;
        Mock::given(path("/3/movie/11"))
            .respond_with(ResponseTemplate::new(200).set_body_json(fixture("tmdb-movie.json")))
            .mount(&server)
            .await;
        let movie = client(&server, "key").movie(11).await.unwrap();
        assert_eq!(movie.summary.title, "Star Wars");
        assert_eq!(movie.summary.year, Some(1977));
        assert_eq!(movie.imdb_id.as_deref(), Some("tt0076759"));
        assert!(!movie.cast.is_empty() && movie.cast.len() <= MAX_CAST);
        // TMDB's documented example for `recommendations` is `{}`.
        assert!(movie.recommendations.is_empty());
        assert!(!movie.similar.is_empty());
        assert!(
            movie
                .alternative_titles
                .contains(&"Podziemny krąg".to_string())
        );
    }

    #[tokio::test]
    async fn reads_series_seasons_and_episodes() {
        let server = MockServer::start().await;
        Mock::given(path("/3/tv/1399"))
            .respond_with(ResponseTemplate::new(200).set_body_json(fixture("tmdb-series.json")))
            .mount(&server)
            .await;
        Mock::given(path("/3/tv/1399/season/1"))
            .respond_with(ResponseTemplate::new(200).set_body_json(fixture("tmdb-season.json")))
            .mount(&server)
            .await;
        let client = client(&server, "key");
        let series = client.series(1399).await.unwrap();
        assert_eq!(series.summary.title, "Game of Thrones");
        assert_eq!(series.imdb_id.as_deref(), Some("tt0944947"));
        assert!(
            series
                .seasons
                .iter()
                .any(|season| season.number == 1 && season.episode_count == 10)
        );

        let season = client.season(1399, 1).await.unwrap();
        assert_eq!(season.episodes.len(), 10);
        assert_eq!(season.aired_episodes("2011-05-01").len(), 3);
    }

    #[tokio::test]
    async fn reads_people_and_mixed_search_results() {
        let server = MockServer::start().await;
        Mock::given(path("/3/person/31"))
            .respond_with(ResponseTemplate::new(200).set_body_json(fixture("tmdb-person.json")))
            .mount(&server)
            .await;
        Mock::given(path("/3/search/multi"))
            .and(query_param("query", "star wars"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(fixture("tmdb-search-multi.json")),
            )
            .mount(&server)
            .await;
        let client = client(&server, "key");
        let person = client.person(31).await.unwrap();
        assert_eq!(person.name, "Tom Hanks");
        assert!(
            person
                .cast
                .iter()
                .any(|credit| credit.role == "Forrest Gump")
        );

        let hits = client.search(" star wars ", 1).await.unwrap();
        assert!(matches!(&hits.items[0], SearchHit::Title(title) if title.title == "Star Wars"));
    }

    #[tokio::test]
    async fn reports_unknown_titles_as_not_found() {
        let server = MockServer::start().await;
        Mock::given(path("/3/movie/999999"))
            .respond_with(ResponseTemplate::new(404).set_body_json(serde_json::json!({
                "success": false, "status_code": 34,
                "status_message": "The resource you requested could not be found."
            })))
            .mount(&server)
            .await;
        let err = client(&server, "key").movie(999999).await.unwrap_err();
        assert!(err.not_found);
    }
}
