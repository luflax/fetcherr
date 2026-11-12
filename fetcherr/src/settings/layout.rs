//! The discover layout: ordered pages (tabs), each with ordered lists of TMDB titles.

use std::collections::HashSet;

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::error::AppError;
use crate::titles::TitleKind;

const MAX_PAGES: usize = 20;
const MAX_LISTS_PER_PAGE: usize = 30;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct Layout {
    pub pages: Vec<DiscoverPage>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct DiscoverPage {
    pub id: String,
    pub name: String,
    pub lists: Vec<ListDefinition>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct ListDefinition {
    /// Unique across all pages.
    pub id: String,
    pub title: String,
    pub source: ListSource,
    #[serde(default)]
    pub filters: DiscoverFilters,
}

/// Where a list's titles come from on TMDB.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ListSource {
    /// `media` omitted: movies and series together.
    Trending {
        #[serde(default)]
        media: Option<TitleKind>,
        #[serde(default)]
        window: TimeWindow,
    },
    Popular {
        media: TitleKind,
    },
    TopRated {
        media: TitleKind,
    },
    /// Movies in theatres.
    NowPlaying,
    /// Upcoming movies.
    Upcoming,
    /// Series with an episode in the next 7 days.
    OnTheAir,
    /// Series with an episode today.
    AiringToday,
    /// TMDB discover, shaped by the list's filters.
    Discover {
        media: TitleKind,
    },
}

impl ListSource {
    /// The kind of titles the list holds, `None` when mixed.
    pub fn media(self) -> Option<TitleKind> {
        match self {
            Self::Trending { media, .. } => media,
            Self::Popular { media } | Self::TopRated { media } | Self::Discover { media } => {
                Some(media)
            }
            Self::NowPlaying | Self::Upcoming => Some(TitleKind::Movie),
            Self::OnTheAir | Self::AiringToday => Some(TitleKind::Series),
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum TimeWindow {
    Day,
    #[default]
    Week,
}

impl TimeWindow {
    pub fn id(self) -> &'static str {
        match self {
            Self::Day => "day",
            Self::Week => "week",
        }
    }
}

/// Filters for `discover` lists (ignored by the other sources).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(default)]
pub struct DiscoverFilters {
    /// TMDB genre ids; a title needs at least one of them.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub genres: Vec<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub year_from: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub year_to: Option<i32>,
    /// Minimum vote average, 0–10.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_vote: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_votes: Option<u32>,
    /// TMDB `sort_by`, e.g. `popularity.desc` or `vote_average.desc`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sort_by: Option<String>,
    /// ISO 639-1, e.g. `ja`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub original_language: Option<String>,
    /// TMDB watch provider ids (needs `watch_region`).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub watch_providers: Vec<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub watch_region: Option<String>,
}

impl Default for Layout {
    fn default() -> Self {
        let list = |id: &str, title: &str, source| ListDefinition {
            id: id.into(),
            title: title.into(),
            source,
            filters: DiscoverFilters::default(),
        };
        let movie = TitleKind::Movie;
        let series = TitleKind::Series;
        Self {
            pages: vec![
                DiscoverPage {
                    id: "home".into(),
                    name: "Home".into(),
                    lists: vec![
                        list(
                            "trending",
                            "Trending",
                            ListSource::Trending {
                                media: None,
                                window: TimeWindow::Week,
                            },
                        ),
                        list(
                            "popular-movies",
                            "Popular movies",
                            ListSource::Popular { media: movie },
                        ),
                        list(
                            "popular-series",
                            "Popular series",
                            ListSource::Popular { media: series },
                        ),
                        list("upcoming-movies", "Upcoming movies", ListSource::Upcoming),
                    ],
                },
                DiscoverPage {
                    id: "movies".into(),
                    name: "Movies".into(),
                    lists: vec![
                        list("now-playing", "In theatres", ListSource::NowPlaying),
                        list(
                            "trending-movies",
                            "Trending movies",
                            ListSource::Trending {
                                media: Some(movie),
                                window: TimeWindow::Week,
                            },
                        ),
                        list(
                            "top-rated-movies",
                            "Top rated movies",
                            ListSource::TopRated { media: movie },
                        ),
                    ],
                },
                DiscoverPage {
                    id: "series".into(),
                    name: "Series".into(),
                    lists: vec![
                        list("airing-today", "Airing today", ListSource::AiringToday),
                        list("on-the-air", "On the air", ListSource::OnTheAir),
                        list(
                            "trending-series",
                            "Trending series",
                            ListSource::Trending {
                                media: Some(series),
                                window: TimeWindow::Week,
                            },
                        ),
                        list(
                            "top-rated-series",
                            "Top rated series",
                            ListSource::TopRated { media: series },
                        ),
                    ],
                },
            ],
        }
    }
}

impl Layout {
    pub fn page(&self, id: &str) -> Option<&DiscoverPage> {
        self.pages.iter().find(|page| page.id == id)
    }

    pub fn list(&self, id: &str) -> Option<&ListDefinition> {
        self.pages
            .iter()
            .flat_map(|page| &page.lists)
            .find(|list| list.id == id)
    }
}

/// A layout sent by a client. Missing ids are generated from the names.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct LayoutInput {
    pub pages: Vec<PageInput>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct PageInput {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    pub name: String,
    #[serde(default)]
    pub lists: Vec<ListInput>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct ListInput {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    pub title: String,
    pub source: ListSource,
    #[serde(default)]
    pub filters: DiscoverFilters,
}

impl LayoutInput {
    pub fn validated(self) -> Result<Layout, AppError> {
        if self.pages.len() > MAX_PAGES {
            return Err(AppError::Invalid(format!("at most {MAX_PAGES} pages")));
        }
        let mut page_ids = UniqueIds::default();
        let mut list_ids = UniqueIds::default();
        let pages = self
            .pages
            .into_iter()
            .map(|page| {
                let name = required("page name", &page.name)?;
                if page.lists.len() > MAX_LISTS_PER_PAGE {
                    return Err(AppError::Invalid(format!(
                        "page '{name}' has more than {MAX_LISTS_PER_PAGE} lists"
                    )));
                }
                let lists = page
                    .lists
                    .into_iter()
                    .map(|list| {
                        let title = required("list title", &list.title)?;
                        Ok(ListDefinition {
                            id: list_ids.claim(list.id.as_deref(), &title)?,
                            filters: validated_filters(list.filters)?,
                            source: list.source,
                            title,
                        })
                    })
                    .collect::<Result<_, AppError>>()?;
                Ok(DiscoverPage {
                    id: page_ids.claim(page.id.as_deref(), &name)?,
                    name,
                    lists,
                })
            })
            .collect::<Result<_, AppError>>()?;
        Ok(Layout { pages })
    }
}

impl From<&Layout> for LayoutInput {
    fn from(layout: &Layout) -> Self {
        Self {
            pages: layout
                .pages
                .iter()
                .map(|page| PageInput {
                    id: Some(page.id.clone()),
                    name: page.name.clone(),
                    lists: page
                        .lists
                        .iter()
                        .map(|list| ListInput {
                            id: Some(list.id.clone()),
                            title: list.title.clone(),
                            source: list.source,
                            filters: list.filters.clone(),
                        })
                        .collect(),
                })
                .collect(),
        }
    }
}

fn required(what: &str, value: &str) -> Result<String, AppError> {
    let value = value.trim();
    if value.is_empty() {
        Err(AppError::Invalid(format!("{what} is required")))
    } else {
        Ok(value.to_string())
    }
}

fn validated_filters(filters: DiscoverFilters) -> Result<DiscoverFilters, AppError> {
    if filters
        .min_vote
        .is_some_and(|vote| !(0.0..=10.0).contains(&vote))
    {
        return Err(AppError::Invalid(
            "minimum vote must be between 0 and 10".into(),
        ));
    }
    if let (Some(from), Some(to)) = (filters.year_from, filters.year_to)
        && from > to
    {
        return Err(AppError::Invalid("year range is reversed".into()));
    }
    let blank_to_none = |value: Option<String>| {
        value
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty())
    };
    Ok(DiscoverFilters {
        sort_by: blank_to_none(filters.sort_by),
        original_language: blank_to_none(filters.original_language),
        watch_region: blank_to_none(filters.watch_region).map(|region| region.to_ascii_uppercase()),
        ..filters
    })
}

/// Hands out unique slug ids: the requested one, or one derived from a name.
#[derive(Default)]
struct UniqueIds(HashSet<String>);

impl UniqueIds {
    fn claim(&mut self, requested: Option<&str>, name: &str) -> Result<String, AppError> {
        if let Some(id) = requested.map(str::trim).filter(|id| !id.is_empty()) {
            if !is_slug(id) {
                return Err(AppError::Invalid(format!(
                    "id '{id}' may only contain a-z, 0-9 and '-'"
                )));
            }
            if !self.0.insert(id.to_string()) {
                return Err(AppError::Invalid(format!("id '{id}' is used twice")));
            }
            return Ok(id.to_string());
        }
        let base = slugify(name);
        let mut candidate = base.clone();
        let mut suffix = 2;
        while !self.0.insert(candidate.clone()) {
            candidate = format!("{base}-{suffix}");
            suffix += 1;
        }
        Ok(candidate)
    }
}

fn is_slug(id: &str) -> bool {
    id.bytes()
        .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

fn slugify(name: &str) -> String {
    let slug = crate::release_name::normalize_title(name).replace(' ', "-");
    if slug.is_empty() { "list".into() } else { slug }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn list(id: Option<&str>, title: &str) -> ListInput {
        ListInput {
            id: id.map(Into::into),
            title: title.into(),
            source: ListSource::Popular {
                media: TitleKind::Movie,
            },
            filters: DiscoverFilters::default(),
        }
    }

    #[test]
    fn generates_unique_ids_from_names() {
        let input = LayoutInput {
            pages: vec![PageInput {
                id: None,
                name: "Kids & Family".into(),
                lists: vec![list(None, "Popular"), list(None, "Popular")],
            }],
        };
        let layout = input.validated().unwrap();
        assert_eq!(layout.pages[0].id, "kids-and-family");
        assert_eq!(layout.pages[0].lists[0].id, "popular");
        assert_eq!(layout.pages[0].lists[1].id, "popular-2");
    }

    #[test]
    fn rejects_duplicate_ids_and_blank_names() {
        let duplicate = LayoutInput {
            pages: vec![PageInput {
                id: Some("home".into()),
                name: "Home".into(),
                lists: vec![list(Some("a"), "A"), list(Some("a"), "B")],
            }],
        };
        assert!(duplicate.validated().is_err());
        let blank = LayoutInput {
            pages: vec![PageInput {
                id: None,
                name: " ".into(),
                lists: vec![],
            }],
        };
        assert!(blank.validated().is_err());
    }

    #[test]
    fn default_layout_round_trips_through_input() {
        let layout = Layout::default();
        assert_eq!(LayoutInput::from(&layout).validated().unwrap(), layout);
        assert!(layout.list("trending").is_some());
    }

    #[test]
    fn list_sources_use_a_kind_tag() {
        let json = serde_json::to_value(ListSource::Trending {
            media: None,
            window: TimeWindow::Day,
        })
        .unwrap();
        assert_eq!(
            json,
            serde_json::json!({"kind": "trending", "media": null, "window": "day"})
        );
    }
}
