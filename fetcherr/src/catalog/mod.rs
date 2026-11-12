//! Browsing: TMDB titles turned into cards with availability (Jellyfin + requests), the
//! configured discover layout, title/person details, search and ratings.

mod availability;
mod ratings;
mod views;

use std::collections::{BTreeSet, HashMap};
use std::sync::Arc;
use std::time::Duration;

use sqlx::SqlitePool;

use crate::cache::TtlCache;
use crate::clock;
use crate::domain::Page;
use crate::error::AppError;
use crate::integrations::SharedIntegrations;
use crate::library::{Library, LibraryIndex};
use crate::metadata::{CastMember, Credit, SearchHit, SeasonInfo, TitleSummary};
use crate::requests::{Requests, TitleRequest};
use crate::settings::SettingsStore;
use crate::titles::{Card, ImageKind, Images, TitleKey, TitleKind};

pub use views::{
    CastMemberView, CreditCard, DiscoverListView, DiscoverPageView, EpisodeView, ImdbScore,
    MovieView, PageTab, PersonCard, PersonView, RatingsView, RelatedList, SearchResult, SeasonCard,
    SeasonView, SeriesView, TmdbScore,
};

/// How long a computed "every aired episode is present" answer is reused for series cards.
const COMPLETENESS_TTL: Duration = Duration::from_secs(30 * 60);

#[derive(Clone)]
pub struct Catalog {
    integrations: Arc<SharedIntegrations>,
    settings: Arc<SettingsStore>,
    library: Library,
    requests: Requests,
    pool: SqlitePool,
    /// Series tmdb id → whether all aired episodes are in the library (from detail views).
    complete_series: Arc<TtlCache<u32, bool>>,
}

/// What every card of one response needs, gathered once.
struct CardContext {
    index: Arc<LibraryIndex>,
    requests: HashMap<TitleKey, Vec<TitleRequest>>,
}

impl Catalog {
    pub fn new(
        integrations: Arc<SharedIntegrations>,
        settings: Arc<SettingsStore>,
        library: Library,
        requests: Requests,
        pool: SqlitePool,
    ) -> Self {
        Self {
            integrations,
            settings,
            library,
            requests,
            pool,
            complete_series: Arc::new(TtlCache::new(10_000)),
        }
    }

    pub fn pages(&self) -> Vec<PageTab> {
        self.settings
            .get()
            .layout
            .pages
            .iter()
            .map(|page| PageTab {
                id: page.id.clone(),
                name: page.name.clone(),
            })
            .collect()
    }

    /// A page with the first page of each of its lists. A failing list comes back empty with
    /// its error, so one bad list does not hide the others.
    pub async fn page(&self, id: &str) -> Result<DiscoverPageView, AppError> {
        let settings = self.settings.get();
        let page = settings
            .layout
            .page(id)
            .ok_or_else(|| AppError::NotFound(format!("page '{id}'")))?;
        let integrations = self.integrations.current();
        let tmdb = integrations.tmdb()?;
        let context = self.context().await?;
        let fetches = page
            .lists
            .iter()
            .map(|list| tmdb.list(list.source, &list.filters, 1));
        let results = futures_util::future::join_all(fetches).await;
        let lists = page
            .lists
            .iter()
            .zip(results)
            .map(|(list, result)| {
                let (cards, error) = match result {
                    Ok(titles) => (titles.map(|title| self.card(&title, &context)), None),
                    Err(err) => (Page::empty(1), Some(err.to_string())),
                };
                DiscoverListView {
                    id: list.id.clone(),
                    title: list.title.clone(),
                    source: list.source,
                    cards,
                    error,
                }
            })
            .collect();
        Ok(DiscoverPageView {
            id: page.id.clone(),
            name: page.name.clone(),
            lists,
        })
    }

    pub async fn list(&self, id: &str, page: u32) -> Result<Page<Card>, AppError> {
        let settings = self.settings.get();
        let list = settings
            .layout
            .list(id)
            .ok_or_else(|| AppError::NotFound(format!("list '{id}'")))?;
        let integrations = self.integrations.current();
        let titles = integrations
            .tmdb()?
            .list(list.source, &list.filters, page)
            .await?;
        let context = self.context().await?;
        Ok(titles.map(|title| self.card(&title, &context)))
    }

    pub async fn search(&self, query: &str, page: u32) -> Result<Page<SearchResult>, AppError> {
        if query.trim().is_empty() {
            return Err(AppError::Invalid("query is empty".into()));
        }
        let integrations = self.integrations.current();
        let hits = integrations.tmdb()?.search(query, page).await?;
        let context = self.context().await?;
        Ok(hits.map(|hit| match hit {
            SearchHit::Title(title) => SearchResult::Title(self.card(&title, &context)),
            SearchHit::Person(person) => SearchResult::Person(PersonCard {
                key: format!("person:{}", person.tmdb_id),
                tmdb_id: person.tmdb_id,
                name: person.name,
                profile: Images::tmdb(ImageKind::Profile, person.profile_path.as_deref()),
                known_for_department: person.known_for_department,
                known_for: person
                    .known_for
                    .iter()
                    .map(|title| self.card(title, &context))
                    .collect(),
            }),
        }))
    }

    pub async fn movie(&self, tmdb_id: u32) -> Result<MovieView, AppError> {
        let integrations = self.integrations.current();
        let movie = integrations.tmdb()?.movie(tmdb_id).await?;
        let context = self.context().await?;
        let key = movie.summary.key();
        Ok(MovieView {
            card: self.card(&movie.summary, &context),
            tagline: movie.tagline,
            runtime: movie.runtime,
            release_date: movie.release_date,
            genres: movie.genres,
            imdb_id: movie.imdb_id,
            vote_count: movie.vote_count,
            cast: cast_views(movie.cast),
            requests: self.requests.for_title(key).await?,
            related: self.related(&movie.recommendations, &movie.similar, &context),
        })
    }

    pub async fn series(&self, tmdb_id: u32) -> Result<SeriesView, AppError> {
        let integrations = self.integrations.current();
        let tmdb = integrations.tmdb()?;
        let series = tmdb.series(tmdb_id).await?;
        let context = self.context().await?;
        let key = series.summary.key();
        let requests = context.requests.get(&key).cloned().unwrap_or_default();
        let today = clock::today();

        // Only the latest season can still be airing; the others aired in full.
        let latest = series
            .seasons
            .iter()
            .filter(|season| season.number > 0)
            .map(|season| season.number)
            .max();
        let latest_aired = match latest {
            Some(number) => tmdb
                .season(tmdb_id, number)
                .await
                .ok()
                .map(|season| season.aired_episodes(&today)),
            None => None,
        };
        let seasons: Vec<SeasonCard> = series
            .seasons
            .iter()
            .map(|season| {
                let aired: BTreeSet<u32> = match (&latest_aired, Some(season.number) == latest) {
                    (Some(aired), true) => aired.clone(),
                    _ if season
                        .air_date
                        .as_deref()
                        .is_some_and(|date| date <= today.as_str()) =>
                    {
                        (1..=season.episode_count).collect()
                    }
                    _ => BTreeSet::new(),
                };
                let present = context.index.episodes(tmdb_id, season.number);
                SeasonCard {
                    number: season.number,
                    name: season.name.clone(),
                    overview: season.overview.clone(),
                    air_date: season.air_date.clone(),
                    poster: Images::tmdb(ImageKind::Poster, season.poster_path.as_deref()),
                    episode_count: season.episode_count,
                    aired_episodes: aired.len() as u32,
                    available_episodes: aired.intersection(&present).count() as u32,
                    availability: availability::season(&aired, &present, season.number, &requests),
                    jellyfin_item_id: context
                        .index
                        .season(tmdb_id, season.number)
                        .and_then(|s| s.item_id.clone()),
                }
            })
            .collect();

        let regular: Vec<&SeasonCard> = seasons.iter().filter(|season| season.number > 0).collect();
        let complete = !regular.is_empty()
            && regular
                .iter()
                .all(|season| season.available_episodes == season.aired_episodes);
        self.complete_series
            .insert(tmdb_id, complete, COMPLETENESS_TTL);

        Ok(SeriesView {
            card: self.card(&series.summary, &context),
            tagline: series.tagline,
            first_air_date: series.first_air_date,
            status: series.status,
            genres: series.genres,
            imdb_id: series.imdb_id,
            vote_count: series.vote_count,
            cast: cast_views(series.cast),
            seasons,
            requests: self.requests.for_title(key).await?,
            related: self.related(&series.recommendations, &series.similar, &context),
        })
    }

    pub async fn season(&self, tmdb_id: u32, number: u32) -> Result<SeasonView, AppError> {
        let integrations = self.integrations.current();
        let season: SeasonInfo = integrations.tmdb()?.season(tmdb_id, number).await?;
        let context = self.context().await?;
        let key = TitleKey::new(TitleKind::Series, tmdb_id);
        let requests = context.requests.get(&key).cloned().unwrap_or_default();
        let library_season = context.index.season(tmdb_id, number);
        let present = context.index.episodes(tmdb_id, number);
        let aired = season.aired_episodes(&clock::today());
        let request_states = self.requests.for_title(key).await?;

        let episodes = season
            .episodes
            .iter()
            .map(|episode| {
                let item_id = library_season
                    .and_then(|s| s.episodes.get(&episode.number))
                    .cloned();
                EpisodeView {
                    number: episode.number,
                    name: episode.name.clone(),
                    overview: episode.overview.clone(),
                    air_date: episode.air_date.clone(),
                    still: Images::tmdb(ImageKind::Still, episode.still_path.as_deref()),
                    runtime: episode.runtime,
                    vote_average: episode.vote_average,
                    availability: availability::episode(
                        item_id.is_some(),
                        number,
                        episode.number,
                        &requests,
                    ),
                    jellyfin_item_id: item_id,
                    request_state: request_states
                        .iter()
                        .rev()
                        .find(|request| request.scope.covers_episode(number, episode.number))
                        .map(|request| request.state),
                }
            })
            .collect();
        Ok(SeasonView {
            key: key.to_string(),
            number: season.number,
            name: season.name,
            overview: season.overview,
            air_date: season.air_date,
            poster: Images::tmdb(ImageKind::Poster, season.poster_path.as_deref()),
            availability: availability::season(&aired, &present, number, &requests),
            jellyfin_item_id: library_season.and_then(|s| s.item_id.clone()),
            episodes,
            requests: request_states
                .into_iter()
                .filter(|request| request.scope.season() == Some(number))
                .collect(),
        })
    }

    pub async fn person(&self, tmdb_id: u32) -> Result<PersonView, AppError> {
        let integrations = self.integrations.current();
        let person = integrations.tmdb()?.person(tmdb_id).await?;
        let context = self.context().await?;
        Ok(PersonView {
            tmdb_id: person.tmdb_id,
            name: person.name,
            biography: person.biography,
            birthday: person.birthday,
            deathday: person.deathday,
            place_of_birth: person.place_of_birth,
            profile: Images::tmdb(ImageKind::Profile, person.profile_path.as_deref()),
            known_for_department: person.known_for_department,
            cast: self.credit_cards(person.cast, &context),
            crew: self.credit_cards(person.crew, &context),
        })
    }

    pub async fn ratings(&self, key: TitleKey) -> Result<RatingsView, AppError> {
        let integrations = self.integrations.current();
        let tmdb = integrations.tmdb()?;
        let (summary, votes, imdb_id) = match key.kind {
            TitleKind::Movie => {
                let movie = tmdb.movie(key.tmdb_id).await?;
                (movie.summary, movie.vote_count, movie.imdb_id)
            }
            TitleKind::Series => {
                let series = tmdb.series(key.tmdb_id).await?;
                (series.summary, series.vote_count, series.imdb_id)
            }
        };
        let external = match (imdb_id, integrations.omdb()) {
            (Some(imdb_id), Ok(omdb)) => ratings::cached(&self.pool, omdb, &imdb_id).await,
            _ => None,
        };
        let external = external.unwrap_or_default();
        Ok(RatingsView {
            tmdb: TmdbScore {
                score: summary.vote_average,
                votes,
            },
            imdb: external.imdb.map(|imdb| ImdbScore {
                score: imdb.score,
                votes: imdb.votes,
            }),
            rotten_tomatoes: external.rotten_tomatoes,
            metacritic: external.metacritic,
        })
    }

    async fn context(&self) -> Result<CardContext, AppError> {
        Ok(CardContext {
            index: self.library.index(),
            requests: self.requests.by_title().await?,
        })
    }

    fn card(&self, title: &TitleSummary, context: &CardContext) -> Card {
        let key = title.key();
        let requests = context
            .requests
            .get(&key)
            .map(Vec::as_slice)
            .unwrap_or_default();
        let item_id = context.index.item_id(key).map(str::to_string);
        let availability = match key.kind {
            TitleKind::Movie => availability::movie(item_id.is_some(), requests),
            TitleKind::Series => availability::series(
                item_id.is_some(),
                self.complete_series.get(&title.tmdb_id),
                requests,
            ),
        };
        Card {
            key: key.to_string(),
            media_type: key.kind,
            tmdb_id: title.tmdb_id,
            title: title.title.clone(),
            year: title.year,
            poster: Images::tmdb(ImageKind::Poster, title.poster_path.as_deref()),
            backdrop: Images::tmdb(ImageKind::Backdrop, title.backdrop_path.as_deref()),
            overview: title.overview.clone(),
            vote_average: title.vote_average,
            availability,
            jellyfin_item_id: item_id,
        }
    }

    fn related(
        &self,
        recommendations: &[TitleSummary],
        similar: &[TitleSummary],
        context: &CardContext,
    ) -> Vec<RelatedList> {
        [
            ("recommendations", "Recommended", recommendations),
            ("similar", "Similar", similar),
        ]
        .into_iter()
        .filter(|(_, _, titles)| !titles.is_empty())
        .map(|(id, title, titles)| RelatedList {
            id: id.into(),
            title: title.into(),
            cards: titles
                .iter()
                .map(|summary| self.card(summary, context))
                .collect(),
        })
        .collect()
    }

    /// One card per title (a person can have several roles in one), newest first.
    fn credit_cards(&self, credits: Vec<Credit>, context: &CardContext) -> Vec<CreditCard> {
        let mut seen = std::collections::HashSet::new();
        let mut cards: Vec<CreditCard> = credits
            .into_iter()
            .filter(|credit| seen.insert(credit.title.key()))
            .map(|credit| CreditCard {
                card: self.card(&credit.title, context),
                role: credit.role,
            })
            .collect();
        cards.sort_by_key(|credit| std::cmp::Reverse(credit.card.year));
        cards
    }
}

fn cast_views(cast: Vec<CastMember>) -> Vec<CastMemberView> {
    cast.into_iter()
        .map(|member| CastMemberView {
            tmdb_id: member.tmdb_id,
            name: member.name,
            character: member.character,
            profile: Images::tmdb(ImageKind::Profile, member.profile_path.as_deref()),
        })
        .collect()
}
