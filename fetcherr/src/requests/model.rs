//! Request types: what was asked for, its state, and the views the API returns.

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::domain::{DownloadState, ReleaseId};
use crate::error::AppError;
use crate::quality::Quality;
use crate::release_name::Coverage;
use crate::titles::{Images, TitleKind};

pub type RequestId = i64;

/// The part of a title a request wants.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Scope {
    Movie,
    /// Every aired episode of a season.
    Season {
        season: u32,
    },
    Episodes {
        season: u32,
        episodes: Vec<u32>,
    },
}

impl Scope {
    /// Checks the scope fits the title kind; sorts and dedups episodes.
    pub fn validated(self, kind: TitleKind) -> Result<Self, AppError> {
        match (kind, self) {
            (TitleKind::Movie, Self::Movie) => Ok(Self::Movie),
            (TitleKind::Series, Self::Season { season }) => Ok(Self::Season { season }),
            (
                TitleKind::Series,
                Self::Episodes {
                    season,
                    mut episodes,
                },
            ) => {
                episodes.sort_unstable();
                episodes.dedup();
                if episodes.is_empty() {
                    return Err(AppError::Invalid("pick at least one episode".into()));
                }
                Ok(Self::Episodes { season, episodes })
            }
            (TitleKind::Movie, _) => Err(AppError::Invalid(
                "a movie request has the movie scope".into(),
            )),
            (TitleKind::Series, Self::Movie) => Err(AppError::Invalid(
                "a series request needs a season or episodes scope".into(),
            )),
        }
    }

    /// Canonical text for uniqueness: `movie`, `season:1`, `episodes:1:2,3`.
    pub fn key(&self) -> String {
        match self {
            Self::Movie => "movie".into(),
            Self::Season { season } => format!("season:{season}"),
            Self::Episodes { season, episodes } => {
                let episodes: Vec<String> = episodes.iter().map(u32::to_string).collect();
                format!("episodes:{season}:{}", episodes.join(","))
            }
        }
    }

    pub fn season(&self) -> Option<u32> {
        match self {
            Self::Movie => None,
            Self::Season { season } | Self::Episodes { season, .. } => Some(*season),
        }
    }

    pub fn covers_episode(&self, season: u32, episode: u32) -> bool {
        match self {
            Self::Movie => false,
            Self::Season { season: wanted } => *wanted == season,
            Self::Episodes {
                season: wanted,
                episodes,
            } => *wanted == season && episodes.contains(&episode),
        }
    }

    /// Whether a release with this coverage can serve the scope.
    pub fn accepts(&self, coverage: &Coverage) -> bool {
        match (self, coverage) {
            (Self::Movie, Coverage::Movie) => true,
            (Self::Season { season }, coverage) => coverage.covers_season(*season),
            (
                Self::Episodes { season, episodes },
                Coverage::Episodes {
                    season: s,
                    episodes: found,
                },
            ) => s == season && found.iter().any(|episode| episodes.contains(episode)),
            (Self::Episodes { season, .. }, coverage @ Coverage::SeasonPack { .. }) => {
                coverage.covers_season(*season)
            }
            _ => false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum RequestState {
    /// Waiting for (or running) a release search.
    Searching,
    /// No suitable release yet; searched again every few hours.
    NotFound,
    Downloading,
    Paused,
    Importing,
    Available,
    Failed,
}

impl RequestState {
    pub const ALL: [Self; 7] = [
        Self::Searching,
        Self::NotFound,
        Self::Downloading,
        Self::Paused,
        Self::Importing,
        Self::Available,
        Self::Failed,
    ];

    pub fn id(self) -> &'static str {
        match self {
            Self::Searching => "searching",
            Self::NotFound => "not_found",
            Self::Downloading => "downloading",
            Self::Paused => "paused",
            Self::Importing => "importing",
            Self::Available => "available",
            Self::Failed => "failed",
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|state| state.id() == id)
    }

    /// Still on its way to the library.
    pub fn is_active(self) -> bool {
        !matches!(self, Self::Available | Self::Failed)
    }

    /// Torrents are in qBittorrent for it.
    pub fn is_transferring(self) -> bool {
        matches!(self, Self::Downloading | Self::Paused | Self::Importing)
    }
}

/// What happened to one torrent of a request.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum TorrentState {
    Downloading,
    Importing,
    Imported,
    /// Downloaded, but the import failed; a retry imports it again.
    ImportFailed,
    Failed,
}

impl TorrentState {
    pub fn id(self) -> &'static str {
        match self {
            Self::Downloading => "downloading",
            Self::Importing => "importing",
            Self::Imported => "imported",
            Self::ImportFailed => "import_failed",
            Self::Failed => "failed",
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        [
            Self::Downloading,
            Self::Importing,
            Self::Imported,
            Self::ImportFailed,
            Self::Failed,
        ]
        .into_iter()
        .find(|state| state.id() == id)
    }

    /// Will provide (or provided) its files.
    pub fn is_useful(self) -> bool {
        self != Self::Failed
    }
}

/// `POST /api/requests` body.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct NewRequest {
    pub media_type: TitleKind,
    pub tmdb_id: u32,
    pub scope: Scope,
    /// Only this quality, instead of the media type's ranking.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub quality: Option<Quality>,
    /// A release from `GET /api/{movies|series}/{tmdb_id}/releases`, instead of picking one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub release_id: Option<ReleaseId>,
}

#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
pub struct RequestSummary {
    pub id: RequestId,
    /// `movie:<tmdb_id>` or `series:<tmdb_id>`.
    pub key: String,
    pub media_type: TitleKind,
    pub tmdb_id: u32,
    pub title: String,
    pub year: Option<i32>,
    pub poster: Option<Images>,
    pub scope: Scope,
    pub quality: Option<Quality>,
    pub state: RequestState,
    /// Why it failed, or what is still missing.
    pub error: Option<String>,
    /// Fraction from 0.0 to 1.0 over all of its torrents.
    pub progress: f64,
    /// RFC 3339.
    pub created_at: String,
    /// RFC 3339.
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
pub struct RequestDetail {
    #[serde(flatten)]
    pub summary: RequestSummary,
    pub torrents: Vec<RequestTorrent>,
    pub files: Vec<ImportedFileView>,
}

#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
pub struct RequestTorrent {
    /// Torrent info-hash (same as `GET /api/downloads/{id}`).
    pub id: String,
    pub release_title: String,
    pub quality: Quality,
    pub size: u64,
    /// What Fetcherr takes from it for this request.
    pub coverage: Coverage,
    pub state: TorrentState,
    pub error: Option<String>,
    pub progress: f64,
    pub dl_speed: u64,
    pub eta_secs: Option<u64>,
    /// Live qBittorrent state, absent once imported or when unknown.
    pub download_state: Option<DownloadState>,
}

#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
pub struct ImportedFileView {
    /// Path in the library, as Fetcherr sees it.
    pub path: String,
    pub season: Option<u32>,
    pub episode: Option<u32>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_scopes_against_the_kind() {
        assert!(Scope::Movie.validated(TitleKind::Series).is_err());
        assert!(
            Scope::Season { season: 1 }
                .validated(TitleKind::Movie)
                .is_err()
        );
        let episodes = Scope::Episodes {
            season: 1,
            episodes: vec![3, 1, 3],
        }
        .validated(TitleKind::Series)
        .unwrap();
        assert_eq!(episodes.key(), "episodes:1:1,3");
        assert!(
            Scope::Episodes {
                season: 1,
                episodes: vec![]
            }
            .validated(TitleKind::Series)
            .is_err()
        );
    }

    #[test]
    fn accepts_releases_that_serve_the_scope() {
        let season = Scope::Season { season: 2 };
        assert!(season.accepts(&Coverage::SeasonPack {
            seasons: vec![1, 2],
            complete: false
        }));
        assert!(season.accepts(&Coverage::Episodes {
            season: 2,
            episodes: vec![5]
        }));
        assert!(!season.accepts(&Coverage::Movie));
        let episodes = Scope::Episodes {
            season: 1,
            episodes: vec![4],
        };
        assert!(!episodes.accepts(&Coverage::Episodes {
            season: 1,
            episodes: vec![5]
        }));
        assert!(Scope::Movie.accepts(&Coverage::Movie));
    }
}
