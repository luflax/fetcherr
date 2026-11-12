//! Puts finished downloads in the library: picks the wanted video files (and their subtitles),
//! names them Jellyfin-style and hardlinks, copies or moves them into place.

pub mod naming;

use std::collections::BTreeSet;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use crate::domain::{Download, DownloadFile};
use crate::error::AppError;
use crate::integrations::Integrations;
use crate::release_name::{self, Coverage};
use crate::settings::{ImportMode, LibrarySettings, PathMapping};
use crate::titles::TitleKind;
use naming::NameValues;

const VIDEO_EXTENSIONS: [&str; 9] = [
    "mkv", "mp4", "avi", "m4v", "ts", "m2ts", "webm", "mov", "wmv",
];
const SUBTITLE_EXTENSIONS: [&str; 5] = ["srt", "ass", "ssa", "sub", "vtt"];
const EXTRA_FOLDERS: [&str; 6] = [
    "sample",
    "samples",
    "extras",
    "featurettes",
    "behind the scenes",
    "trailers",
];

/// The title being imported.
#[derive(Debug, Clone)]
pub struct ImportTitle<'a> {
    pub kind: TitleKind,
    pub tmdb_id: u32,
    pub title: &'a str,
    pub year: Option<i32>,
    pub quality: &'a str,
}

/// A file placed in the library.
#[derive(Debug, Clone, PartialEq)]
pub struct ImportedFile {
    pub source: PathBuf,
    pub destination: PathBuf,
    pub season: Option<u32>,
    pub episode: Option<u32>,
}

/// Imports the files of `download` that `wanted` asks for (a movie, or episodes of a season).
pub async fn import_download(
    integrations: &Integrations,
    library: &LibrarySettings,
    title: &ImportTitle<'_>,
    wanted: &Coverage,
    download: &Download,
) -> Result<Vec<ImportedFile>, AppError> {
    let root = library.require_folder(title.kind)?;
    let files = integrations.qbittorrent()?.files(&download.id).await?;
    let selected = select_files(title.kind, &files, wanted);
    if selected.is_empty() {
        return Err(AppError::NotFound(format!(
            "no wanted video file in '{}'",
            download.name
        )));
    }
    let save_path = map_path(&download.save_path, &library.path_mappings);
    let mut imported = Vec::new();
    for file in selected {
        let source = join_relative(&save_path, &file.name);
        if !tokio::fs::try_exists(&source).await.unwrap_or(false) {
            return Err(AppError::NotFound(format!(
                "{} (does Fetcherr see qBittorrent's folders? Check the path mappings)",
                source.display()
            )));
        }
        let destination = Path::new(root).join(destination(library, title, &file));
        place(&source, &destination, library.import_mode).await?;
        imported.push(ImportedFile {
            source,
            destination,
            season: file.season,
            episode: file.episodes.first().copied(),
        });
    }
    Ok(imported)
}

/// One file to import and what it holds.
#[derive(Debug, Clone, PartialEq)]
struct SelectedFile {
    name: String,
    season: Option<u32>,
    episodes: Vec<u32>,
    /// Text after the video's name, e.g. `.en.forced` for `Movie.en.forced.srt`.
    subtitle_suffix: Option<String>,
}

/// Rewrites the first matching prefix of a qBittorrent path.
pub fn map_path(remote: &str, mappings: &[PathMapping]) -> PathBuf {
    let remote_normalized = remote.replace('\\', "/");
    for mapping in mappings {
        let prefix = mapping.remote.replace('\\', "/");
        let prefix = prefix.trim_end_matches('/');
        if let Some(rest) = remote_normalized.strip_prefix(prefix)
            && (rest.is_empty() || rest.starts_with('/'))
        {
            return join_relative(Path::new(&mapping.local), rest);
        }
    }
    PathBuf::from(remote)
}

/// Joins a `/`-separated relative path, ignoring `..` and empty parts.
fn join_relative(base: &Path, relative: &str) -> PathBuf {
    relative
        .split('/')
        .filter(|part| !part.is_empty() && *part != "." && *part != "..")
        .fold(base.to_path_buf(), |path, part| path.join(part))
}

fn select_files(kind: TitleKind, files: &[DownloadFile], wanted: &Coverage) -> Vec<SelectedFile> {
    let videos: Vec<&DownloadFile> = files
        .iter()
        .filter(|file| {
            !file.skipped && has_extension(&file.name, &VIDEO_EXTENSIONS) && !is_extra(&file.name)
        })
        .collect();
    let chosen: Vec<SelectedFile> = match (kind, wanted) {
        (TitleKind::Movie, _) => videos
            .iter()
            .max_by_key(|file| file.size)
            .map(|file| SelectedFile {
                name: file.name.clone(),
                season: None,
                episodes: Vec::new(),
                subtitle_suffix: None,
            })
            .into_iter()
            .collect(),
        (TitleKind::Series, Coverage::Episodes { season, episodes }) => {
            let wanted: BTreeSet<u32> = episodes.iter().copied().collect();
            let mut taken = BTreeSet::new();
            let mut chosen = Vec::new();
            let mut by_size = videos.clone();
            by_size.sort_by_key(|file| std::cmp::Reverse(file.size));
            for file in by_size {
                let Some((found_season, found)) = episodes_in(&file.name) else {
                    continue;
                };
                let new: Vec<u32> = found
                    .iter()
                    .copied()
                    .filter(|e| !taken.contains(e))
                    .collect();
                if found_season == *season
                    && !new.is_empty()
                    && new.iter().all(|e| wanted.contains(e))
                {
                    taken.extend(found.iter().copied());
                    chosen.push(SelectedFile {
                        name: file.name.clone(),
                        season: Some(*season),
                        episodes: found,
                        subtitle_suffix: None,
                    });
                }
            }
            chosen.sort_by_key(|file| file.episodes.first().copied());
            chosen
        }
        (TitleKind::Series, _) => Vec::new(),
    };
    let subtitles = chosen
        .iter()
        .flat_map(|video| subtitles_for(video, files, chosen.len() == 1))
        .collect::<Vec<_>>();
    chosen.into_iter().chain(subtitles).collect()
}

/// Season and episodes of a file, read from its name, else from its folder.
fn episodes_in(path: &str) -> Option<(u32, Vec<u32>)> {
    path.rsplit('/')
        .find_map(|part| match release_name::parse(part).coverage {
            Coverage::Episodes { season, episodes } => Some((season, episodes)),
            _ => None,
        })
}

/// Subtitles next to a video: named after it, or (for a lone video) any in the download.
fn subtitles_for(
    video: &SelectedFile,
    files: &[DownloadFile],
    lone_video: bool,
) -> Vec<SelectedFile> {
    let video_stem = stem(&video.name);
    files
        .iter()
        .filter(|file| !file.skipped && has_extension(&file.name, &SUBTITLE_EXTENSIONS))
        .filter_map(|file| {
            let subtitle_stem = stem(&file.name);
            let suffix = if let Some(rest) = subtitle_stem.strip_prefix(video_stem) {
                rest.to_string()
            } else if video.season.is_some() {
                let (season, episodes) = episodes_in(&file.name)?;
                if Some(season) != video.season || episodes != video.episodes {
                    return None;
                }
                language_suffix(subtitle_stem)
            } else if lone_video {
                language_suffix(subtitle_stem)
            } else {
                return None;
            };
            Some(SelectedFile {
                name: file.name.clone(),
                season: video.season,
                episodes: video.episodes.clone(),
                subtitle_suffix: Some(suffix),
            })
        })
        .collect()
}

/// `.en` from `English.en` or `2_English`: a short trailing language code, if any.
fn language_suffix(stem: &str) -> String {
    stem.rsplit(['.', '_', ' '])
        .next()
        .filter(|code| {
            (2..=3).contains(&code.len()) && code.bytes().all(|b| b.is_ascii_alphabetic())
        })
        .map(|code| format!(".{}", code.to_ascii_lowercase()))
        .unwrap_or_default()
}

fn destination(library: &LibrarySettings, title: &ImportTitle<'_>, file: &SelectedFile) -> PathBuf {
    let naming = &library.naming;
    let values = NameValues {
        title: title.title,
        year: title.year,
        tmdb_id: title.tmdb_id,
        season: file.season,
        episodes: &file.episodes,
        quality: title.quality,
    };
    let extension = extension(&file.name);
    let suffix = file.subtitle_suffix.as_deref().unwrap_or("");
    match title.kind {
        TitleKind::Movie => {
            let name = naming::render(&naming.movie_file, &values);
            PathBuf::from(naming::render(&naming.movie_folder, &values))
                .join(format!("{name}{suffix}.{extension}"))
        }
        TitleKind::Series => {
            let name = naming::render(&naming.episode_file, &values);
            PathBuf::from(naming::render(&naming.series_folder, &values))
                .join(naming::render(&naming.season_folder, &values))
                .join(format!("{name}{suffix}.{extension}"))
        }
    }
}

/// Hardlinks (or copies, across filesystems) or moves `source` to `destination`.
/// An existing destination is kept: it is the same file imported before.
async fn place(source: &Path, destination: &Path, mode: ImportMode) -> Result<(), AppError> {
    let failed = |action: &str, err: std::io::Error| {
        AppError::Internal(format!(
            "cannot {action} {} to {}: {err}",
            source.display(),
            destination.display()
        ))
    };
    if tokio::fs::try_exists(destination).await.unwrap_or(false) {
        return Ok(());
    }
    if let Some(folder) = destination.parent() {
        tokio::fs::create_dir_all(folder)
            .await
            .map_err(|err| failed("create the folder for", err))?;
    }
    let result = match mode {
        ImportMode::Hardlink => tokio::fs::hard_link(source, destination).await,
        ImportMode::Move => tokio::fs::rename(source, destination).await,
    };
    match result {
        Ok(()) => Ok(()),
        Err(err) if crosses_devices(&err) => {
            tokio::fs::copy(source, destination)
                .await
                .map_err(|err| failed("copy", err))?;
            if mode == ImportMode::Move {
                tokio::fs::remove_file(source)
                    .await
                    .map_err(|err| failed("remove the source after copying", err))?;
            }
            Ok(())
        }
        Err(err) => Err(failed(
            match mode {
                ImportMode::Hardlink => "hardlink",
                ImportMode::Move => "move",
            },
            err,
        )),
    }
}

/// Links and renames only work within one filesystem (EXDEV on Unix, 17 on Windows).
fn crosses_devices(err: &std::io::Error) -> bool {
    err.kind() == ErrorKind::CrossesDevices
        || (cfg!(unix) && err.raw_os_error() == Some(18))
        || (cfg!(windows) && err.raw_os_error() == Some(17))
}

fn file_name(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

fn extension(path: &str) -> String {
    file_name(path)
        .rsplit_once('.')
        .map(|(_, ext)| ext.to_ascii_lowercase())
        .unwrap_or_default()
}

fn stem(path: &str) -> &str {
    let name = file_name(path);
    name.rsplit_once('.').map_or(name, |(stem, _)| stem)
}

fn has_extension(path: &str, extensions: &[&str]) -> bool {
    extensions.contains(&extension(path).as_str())
}

fn is_extra(path: &str) -> bool {
    let lower = path.to_ascii_lowercase();
    let in_extra_folder = lower
        .split('/')
        .rev()
        .skip(1)
        .any(|folder| EXTRA_FOLDERS.contains(&folder));
    in_extra_folder
        || stem(&lower)
            .split(['.', '-', '_', ' '])
            .any(|word| word == "sample")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(name: &str, size: u64) -> DownloadFile {
        DownloadFile {
            name: name.into(),
            size,
            progress: 1.0,
            skipped: false,
        }
    }

    fn names(selected: &[SelectedFile]) -> Vec<&str> {
        selected.iter().map(|file| file.name.as_str()).collect()
    }

    #[test]
    fn maps_qbittorrent_paths() {
        let mappings = vec![PathMapping {
            remote: "/downloads/".into(),
            local: "/share/downloads".into(),
        }];
        assert_eq!(
            map_path("/downloads/movies", &mappings),
            PathBuf::from("/share/downloads/movies")
        );
        assert_eq!(
            map_path("/downloads", &mappings),
            PathBuf::from("/share/downloads")
        );
        assert_eq!(
            map_path("/downloadsX/a", &mappings),
            PathBuf::from("/downloadsX/a")
        );
        assert_eq!(
            join_relative(Path::new("/a"), "../b/./c"),
            PathBuf::from("/a/b/c")
        );
    }

    #[test]
    fn picks_the_main_movie_file_and_its_subtitles() {
        let files = [
            file("Dune.2021/Dune.2021.1080p.mkv", 9_000),
            file("Dune.2021/Sample/dune.sample.mkv", 100),
            file("Dune.2021/Dune.2021.1080p.en.srt", 1),
            file("Dune.2021/Subs/2_English.srt", 1),
            file("Dune.2021/info.nfo", 1),
        ];
        let selected = select_files(TitleKind::Movie, &files, &Coverage::Movie);
        assert_eq!(
            names(&selected),
            [
                "Dune.2021/Dune.2021.1080p.mkv",
                "Dune.2021/Dune.2021.1080p.en.srt",
                "Dune.2021/Subs/2_English.srt"
            ]
        );
        assert_eq!(selected[1].subtitle_suffix.as_deref(), Some(".en"));
        assert_eq!(selected[2].subtitle_suffix.as_deref(), Some(""));
    }

    #[test]
    fn picks_only_wanted_episodes_from_a_pack() {
        let files = [
            file("Show.S01/Show.S01E01.mkv", 10),
            file("Show.S01/Show.S01E02.mkv", 10),
            file("Show.S01/Show.S01E03.mkv", 10),
            file("Show.S01/Show.S01E03.srt", 1),
            file("Show.S01/Featurettes/Show.S01E03.Making.Of.mkv", 1),
        ];
        let wanted = Coverage::Episodes {
            season: 1,
            episodes: vec![2, 3],
        };
        let selected = select_files(TitleKind::Series, &files, &wanted);
        assert_eq!(
            names(&selected),
            [
                "Show.S01/Show.S01E02.mkv",
                "Show.S01/Show.S01E03.mkv",
                "Show.S01/Show.S01E03.srt"
            ]
        );
    }

    #[test]
    fn names_destinations_jellyfin_style() {
        let library = LibrarySettings::default();
        let title = ImportTitle {
            kind: TitleKind::Series,
            tmdb_id: 1399,
            title: "Game of Thrones",
            year: Some(2011),
            quality: "WEB-1080p",
        };
        let episode = SelectedFile {
            name: "x/Game.of.Thrones.S01E02.mkv".into(),
            season: Some(1),
            episodes: vec![2],
            subtitle_suffix: None,
        };
        assert_eq!(
            destination(&library, &title, &episode),
            Path::new("Game of Thrones (2011) [tmdbid-1399]")
                .join("Season 01")
                .join("Game of Thrones S01E02.mkv")
        );
    }

    #[tokio::test]
    async fn hardlinks_and_keeps_existing_files() {
        let dir = std::env::temp_dir().join(format!("fetcherr-place-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let source = dir.join("source.mkv");
        std::fs::write(&source, b"video").unwrap();
        let destination = dir
            .join("Library")
            .join("Movie (2020)")
            .join("Movie (2020).mkv");

        place(&source, &destination, ImportMode::Hardlink)
            .await
            .unwrap();
        place(&source, &destination, ImportMode::Hardlink)
            .await
            .unwrap();

        assert_eq!(std::fs::read(&destination).unwrap(), b"video");
        assert!(source.exists());
        let _ = std::fs::remove_dir_all(dir);
    }
}
