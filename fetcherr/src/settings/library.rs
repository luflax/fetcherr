//! Where finished downloads are imported, how, and under which names.

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::error::AppError;
use crate::import::naming;
use crate::titles::TitleKind;

/// How a finished download reaches the library.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ImportMode {
    /// Keep seeding: the library file is a hardlink (a copy across filesystems).
    #[default]
    Hardlink,
    /// Move the files and remove the torrent from qBittorrent.
    Move,
}

/// Rewrites a qBittorrent path prefix into the same folder as Fetcherr sees it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct PathMapping {
    /// As qBittorrent reports it, e.g. `/downloads`.
    pub remote: String,
    /// As Fetcherr sees it, e.g. `/share/downloads`.
    pub local: String,
}

/// Name templates. Placeholders: `{title}`, `{year}`, `{tmdb_id}`, `{season}`, `{episode}`,
/// `{quality}`; add `:02` for zero padding (`{season:02}`). A multi-episode file renders
/// `{episode:02}` as `01-E02`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(default)]
pub struct NamingSettings {
    pub movie_folder: String,
    pub movie_file: String,
    pub series_folder: String,
    pub season_folder: String,
    pub episode_file: String,
}

impl Default for NamingSettings {
    fn default() -> Self {
        Self {
            movie_folder: "{title} ({year}) [tmdbid-{tmdb_id}]".into(),
            movie_file: "{title} ({year})".into(),
            series_folder: "{title} ({year}) [tmdbid-{tmdb_id}]".into(),
            season_folder: "Season {season:02}".into(),
            episode_file: "{title} S{season:02}E{episode:02}".into(),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(default)]
pub struct LibrarySettings {
    /// Library root for movies, as Fetcherr sees it (e.g. `/media/movies`).
    pub movies_folder: String,
    /// Library root for series.
    pub series_folder: String,
    pub import_mode: ImportMode,
    pub path_mappings: Vec<PathMapping>,
    pub naming: NamingSettings,
}

impl LibrarySettings {
    pub fn folder(&self, kind: TitleKind) -> Option<&str> {
        let folder = match kind {
            TitleKind::Movie => &self.movies_folder,
            TitleKind::Series => &self.series_folder,
        };
        Some(folder.as_str()).filter(|folder| !folder.is_empty())
    }

    /// Cleans and checks settings sent by a client; the input has the same shape.
    pub fn validated(self) -> Result<Self, AppError> {
        let naming = self.naming;
        for (name, template, needs_episode) in [
            ("movie folder", &naming.movie_folder, false),
            ("movie file", &naming.movie_file, false),
            ("series folder", &naming.series_folder, false),
            ("season folder", &naming.season_folder, false),
            ("episode file", &naming.episode_file, true),
        ] {
            naming::check_template(template, needs_episode)
                .map_err(|reason| AppError::Invalid(format!("{name} template: {reason}")))?;
        }
        let path_mappings = self
            .path_mappings
            .into_iter()
            .map(|mapping| PathMapping {
                remote: mapping.remote.trim().to_string(),
                local: mapping.local.trim().to_string(),
            })
            .filter(|mapping| !mapping.remote.is_empty() || !mapping.local.is_empty())
            .map(|mapping| {
                if mapping.remote.is_empty() || mapping.local.is_empty() {
                    Err(AppError::Invalid("a path mapping needs both paths".into()))
                } else {
                    Ok(mapping)
                }
            })
            .collect::<Result<_, _>>()?;
        Ok(Self {
            movies_folder: self.movies_folder.trim().to_string(),
            series_folder: self.series_folder.trim().to_string(),
            import_mode: self.import_mode,
            path_mappings,
            naming: NamingSettings {
                movie_folder: naming.movie_folder.trim().to_string(),
                movie_file: naming.movie_file.trim().to_string(),
                series_folder: naming.series_folder.trim().to_string(),
                season_folder: naming.season_folder.trim().to_string(),
                episode_file: naming.episode_file.trim().to_string(),
            },
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_half_mappings_and_bad_templates() {
        let half = LibrarySettings {
            path_mappings: vec![PathMapping {
                remote: "/downloads".into(),
                local: " ".into(),
            }],
            ..Default::default()
        };
        assert!(half.validated().is_err());

        let mut no_episode = LibrarySettings::default();
        no_episode.naming.episode_file = "{title}".into();
        assert!(no_episode.validated().is_err());

        let blank_rows = LibrarySettings {
            path_mappings: vec![PathMapping {
                remote: String::new(),
                local: String::new(),
            }],
            ..Default::default()
        };
        assert!(blank_rows.validated().unwrap().path_mappings.is_empty());
    }
}
