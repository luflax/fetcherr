//! Movies and series as Fetcherr shows them: identity, images, availability and the card model.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::domain::MediaType;
use crate::error::AppError;

const TMDB_IMAGE_BASE: &str = "https://image.tmdb.org/t/p/";

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, ToSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum TitleKind {
    Movie,
    Series,
}

impl TitleKind {
    pub fn id(self) -> &'static str {
        match self {
            Self::Movie => "movie",
            Self::Series => "series",
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        match id {
            "movie" => Some(Self::Movie),
            "series" => Some(Self::Series),
            _ => None,
        }
    }

    /// The media type whose indexer categories, qualities and folders apply.
    pub fn media_type(self) -> MediaType {
        match self {
            Self::Movie => MediaType::Movies,
            Self::Series => MediaType::Series,
        }
    }
}

impl fmt::Display for TitleKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.id())
    }
}

/// `movie:603` / `series:1399`: a title, unique across media types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TitleKey {
    pub kind: TitleKind,
    pub tmdb_id: u32,
}

impl TitleKey {
    pub fn new(kind: TitleKind, tmdb_id: u32) -> Self {
        Self { kind, tmdb_id }
    }
}

impl fmt::Display for TitleKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.kind, self.tmdb_id)
    }
}

impl FromStr for TitleKey {
    type Err = AppError;

    fn from_str(raw: &str) -> Result<Self, Self::Err> {
        let invalid = || AppError::Invalid(format!("'{raw}' is not a title key like movie:603"));
        let (kind, id) = raw.split_once(':').ok_or_else(invalid)?;
        Ok(Self {
            kind: TitleKind::from_id(kind).ok_or_else(invalid)?,
            tmdb_id: id.parse().map_err(|_| invalid())?,
        })
    }
}

/// What an image is, deciding the TMDB sizes offered.
#[derive(Debug, Clone, Copy)]
pub enum ImageKind {
    Poster,
    Backdrop,
    Profile,
    Still,
}

impl ImageKind {
    /// TMDB sizes for small, medium and large (plus `original`).
    fn sizes(self) -> [&'static str; 3] {
        match self {
            Self::Poster => ["w185", "w342", "w500"],
            Self::Backdrop => ["w300", "w780", "w1280"],
            Self::Profile => ["w45", "w185", "h632"],
            Self::Still => ["w92", "w185", "w300"],
        }
    }
}

/// Ready-to-use image URLs in several sizes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct Images {
    pub small: String,
    pub medium: String,
    pub large: String,
    pub original: String,
}

impl Images {
    /// URLs for a TMDB image path such as `/abc.jpg`; `None` without a path.
    pub fn tmdb(kind: ImageKind, path: Option<&str>) -> Option<Self> {
        let path = path.filter(|path| path.starts_with('/'))?;
        let [small, medium, large] = kind
            .sizes()
            .map(|size| format!("{TMDB_IMAGE_BASE}{size}{path}"));
        Some(Self {
            small,
            medium,
            large,
            original: format!("{TMDB_IMAGE_BASE}original{path}"),
        })
    }
}

/// Whether a title (or season/episode) can be watched, or is on its way.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum Availability {
    Available,
    PartiallyAvailable,
    Downloading,
    Requested,
    Unavailable,
}

/// The one card model used by every list, search result and related title.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct Card {
    /// `movie:<tmdb_id>` or `series:<tmdb_id>`.
    #[schema(example = "movie:603")]
    pub key: String,
    pub media_type: TitleKind,
    pub tmdb_id: u32,
    pub title: String,
    pub year: Option<i32>,
    pub poster: Option<Images>,
    pub backdrop: Option<Images>,
    pub overview: String,
    pub vote_average: f64,
    pub availability: Availability,
    pub jellyfin_item_id: Option<String>,
}

/// The year of a TMDB date (`2021-10-22`), if any.
pub fn year_of(date: Option<&str>) -> Option<i32> {
    date?.get(..4)?.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn title_keys_round_trip() {
        let key: TitleKey = "series:1399".parse().unwrap();
        assert_eq!(key, TitleKey::new(TitleKind::Series, 1399));
        assert_eq!(key.to_string(), "series:1399");
        assert!("person:1".parse::<TitleKey>().is_err());
        assert!("movie:x".parse::<TitleKey>().is_err());
    }

    #[test]
    fn builds_sized_image_urls() {
        let images = Images::tmdb(ImageKind::Poster, Some("/p.jpg")).unwrap();
        assert_eq!(images.small, "https://image.tmdb.org/t/p/w185/p.jpg");
        assert_eq!(images.original, "https://image.tmdb.org/t/p/original/p.jpg");
        assert_eq!(Images::tmdb(ImageKind::Poster, None), None);
    }

    #[test]
    fn reads_the_year_of_a_date() {
        assert_eq!(year_of(Some("2021-10-22")), Some(2021));
        assert_eq!(year_of(Some("")), None);
        assert_eq!(year_of(None), None);
    }
}
