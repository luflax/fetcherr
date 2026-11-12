//! Reads what a release contains from its name: the title, the year and which seasons or
//! episodes it covers (`S01E02`, `S01E02-E04`, `1x02`, `S01`, `Season 1`, `S01-S03`, `Complete`).

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// What part of a title a release (or a file in it) covers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Coverage {
    /// No season or episode marker.
    Movie,
    /// Whole seasons. Empty `seasons` with `complete` means every season.
    SeasonPack {
        seasons: Vec<u32>,
        complete: bool,
    },
    Episodes {
        season: u32,
        episodes: Vec<u32>,
    },
}

impl Coverage {
    pub fn covers_season(&self, season: u32) -> bool {
        match self {
            Self::Movie => false,
            Self::SeasonPack { seasons, complete } => {
                seasons.contains(&season) || (*complete && seasons.is_empty())
            }
            Self::Episodes {
                season: covered, ..
            } => *covered == season,
        }
    }

    pub fn is_pack(&self) -> bool {
        matches!(self, Self::SeasonPack { .. })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ParsedName {
    /// Normalized words before the first marker (see [`normalize_title`]).
    pub title: String,
    pub year: Option<i32>,
    pub coverage: Coverage,
}

pub fn parse(name: &str) -> ParsedName {
    let tokens = tokenize(name);
    let mut title_end = None;
    let mut year = None;
    let mut coverage = None;
    let mut complete = false;

    let mut index = 0;
    while index < tokens.len() {
        let token = tokens[index];
        let mut used = 1;
        let marker = match episode_marker(&tokens[index..]) {
            Some((found, length)) => {
                coverage.get_or_insert(found);
                used = length;
                true
            }
            None if is_release_year(&tokens, index) && title_end.is_none() => {
                year = token.parse().ok();
                true
            }
            None if token.eq_ignore_ascii_case("complete") => {
                complete = true;
                true
            }
            None => is_tag(token),
        };
        if marker && title_end.is_none() {
            title_end = Some(index);
        }
        index += used;
    }

    let title_tokens = &tokens[..title_end.unwrap_or(tokens.len())];
    let coverage = match coverage {
        Some(Coverage::SeasonPack { seasons, .. }) => Coverage::SeasonPack { seasons, complete },
        Some(found) => found,
        None if complete => Coverage::SeasonPack {
            seasons: Vec::new(),
            complete: true,
        },
        None => Coverage::Movie,
    };
    ParsedName {
        title: normalize_title(&title_tokens.join(" ")),
        year,
        coverage,
    }
}

/// Lowercase words: accents folded, `&` read as "and", apostrophes dropped, other punctuation
/// as spaces and a leading "the" ignored. Two titles match when these are equal.
pub fn normalize_title(title: &str) -> String {
    let simple = simplify_title(title);
    match simple.split_once(' ') {
        Some(("the", rest)) => rest.to_string(),
        _ => simple,
    }
}

/// [`normalize_title`] keeping a leading "the": the text to send to indexers.
pub fn simplify_title(title: &str) -> String {
    let mut folded = String::with_capacity(title.len());
    for c in title.chars() {
        match c {
            '&' => folded.push_str(" and "),
            '\'' | '’' | '`' => {}
            c if c.is_alphanumeric() => folded.extend(fold_accent(c).to_lowercase()),
            _ => folded.push(' '),
        }
    }
    folded.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Whether a parsed release title is one of `titles`. A title ending in a year-like number
/// ("Blade Runner 2049") parses as title + year, so that combination counts too.
pub fn title_matches(parsed: &ParsedName, titles: &[String]) -> bool {
    let with_year = parsed.year.map(|year| format!("{} {year}", parsed.title));
    titles
        .iter()
        .map(|title| normalize_title(title))
        .any(|title| {
            !title.is_empty()
                && (title == parsed.title || with_year.as_deref() == Some(title.as_str()))
        })
}

fn fold_accent(c: char) -> char {
    match c {
        'à' | 'á' | 'â' | 'ã' | 'ä' | 'å' => 'a',
        'À' | 'Á' | 'Â' | 'Ã' | 'Ä' | 'Å' => 'A',
        'ç' => 'c',
        'Ç' => 'C',
        'è' | 'é' | 'ê' | 'ë' => 'e',
        'È' | 'É' | 'Ê' | 'Ë' => 'E',
        'ì' | 'í' | 'î' | 'ï' => 'i',
        'Ì' | 'Í' | 'Î' | 'Ï' => 'I',
        'ñ' => 'n',
        'Ñ' => 'N',
        'ò' | 'ó' | 'ô' | 'õ' | 'ö' | 'ø' => 'o',
        'Ò' | 'Ó' | 'Ô' | 'Õ' | 'Ö' | 'Ø' => 'O',
        'ù' | 'ú' | 'û' | 'ü' => 'u',
        'Ù' | 'Ú' | 'Û' | 'Ü' => 'U',
        'ý' | 'ÿ' => 'y',
        'Ý' => 'Y',
        other => other,
    }
}

fn tokenize(name: &str) -> Vec<&str> {
    name.split(|c: char| {
        matches!(
            c,
            '.' | '_' | ' ' | '-' | '(' | ')' | '[' | ']' | '{' | '}' | '+'
        )
    })
    .filter(|token| !token.is_empty())
    .collect()
}

/// A year that is not the first word nor followed by another year (`Blade Runner 2049 2017`).
fn is_release_year(tokens: &[&str], index: usize) -> bool {
    index > 0 && is_year(tokens[index]) && !tokens.get(index + 1).is_some_and(|next| is_year(next))
}

fn is_year(token: &str) -> bool {
    token.len() == 4
        && token
            .parse::<i32>()
            .is_ok_and(|year| (1900..=2099).contains(&year))
}

/// Words that never belong to a title: resolutions, sources and codecs.
fn is_tag(token: &str) -> bool {
    const TAGS: [&str; 22] = [
        "2160p", "1080p", "1080i", "720p", "576p", "480p", "4k", "uhd", "bluray", "bdrip", "brrip",
        "web", "webdl", "webrip", "hdtv", "dvdrip", "remux", "x264", "x265", "h264", "h265",
        "hevc",
    ];
    let lower = token.to_ascii_lowercase();
    TAGS.contains(&lower.as_str())
}

/// A season/episode marker starting at `tokens[0]`, with how many tokens it used.
fn episode_marker(tokens: &[&str]) -> Option<(Coverage, usize)> {
    let first = tokens.first()?;
    if let Some((season, episodes)) = season_episode(first) {
        let mut episodes = episodes;
        let mut used = 1;
        // `S01E02-E04` is split into `S01E02` and `E04`: a range.
        if let Some(last) = tokens.get(1).and_then(|next| e_number(next))
            && let Some(&start) = episodes.last()
            && last > start
        {
            episodes.extend(start + 1..=last);
            used = 2;
        }
        return Some((Coverage::Episodes { season, episodes }, used));
    }
    if let Some(found) = cross_episode(first) {
        return Some(found);
    }
    if let Some(season) = s_number(first) {
        // `S01-S03` is split into `S01` and `S03`: a season range.
        let end = tokens
            .get(1)
            .and_then(|next| s_number(next))
            .filter(|end| *end > season);
        let seasons = (season..=end.unwrap_or(season)).collect();
        let used = if end.is_some() { 2 } else { 1 };
        return Some((
            Coverage::SeasonPack {
                seasons,
                complete: false,
            },
            used,
        ));
    }
    if first.eq_ignore_ascii_case("season")
        && let Some(season) = tokens.get(1).and_then(|next| next.parse::<u32>().ok())
    {
        return Some((
            Coverage::SeasonPack {
                seasons: vec![season],
                complete: false,
            },
            2,
        ));
    }
    None
}

/// `S01E02`, `S01E02E03`: the season and the episodes.
fn season_episode(token: &str) -> Option<(u32, Vec<u32>)> {
    let lower = token.to_ascii_lowercase();
    let rest = lower.strip_prefix('s')?;
    let (season, rest) = leading_number(rest)?;
    if rest.is_empty() {
        return None;
    }
    let mut episodes = Vec::new();
    for part in rest.split('e').skip(1) {
        episodes.push(part.parse().ok()?);
    }
    (rest.starts_with('e') && !episodes.is_empty()).then_some((season, episodes))
}

/// `1x02`.
fn cross_episode(token: &str) -> Option<(Coverage, usize)> {
    let (season, episode) = token
        .to_ascii_lowercase()
        .split_once('x')
        .and_then(|(s, e)| {
            let both_numeric = !s.is_empty() && s.len() <= 2 && e.len() >= 2 && e.len() <= 3;
            both_numeric.then(|| Some((s.parse().ok()?, e.parse().ok()?)))?
        })?;
    Some((
        Coverage::Episodes {
            season,
            episodes: vec![episode],
        },
        1,
    ))
}

/// `S01` alone.
fn s_number(token: &str) -> Option<u32> {
    let lower = token.to_ascii_lowercase();
    let digits = lower.strip_prefix('s')?;
    (!digits.is_empty() && digits.len() <= 3 && digits.bytes().all(|b| b.is_ascii_digit()))
        .then(|| digits.parse().ok())?
}

/// `E04` alone.
fn e_number(token: &str) -> Option<u32> {
    let lower = token.to_ascii_lowercase();
    let digits = lower.strip_prefix('e')?;
    (!digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit()))
        .then(|| digits.parse().ok())?
}

fn leading_number(text: &str) -> Option<(u32, &str)> {
    let end = text
        .find(|c: char| !c.is_ascii_digit())
        .unwrap_or(text.len());
    if end == 0 || end > 3 {
        return None;
    }
    Some((text[..end].parse().ok()?, &text[end..]))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn episodes(season: u32, episodes: &[u32]) -> Coverage {
        Coverage::Episodes {
            season,
            episodes: episodes.to_vec(),
        }
    }

    fn pack(seasons: &[u32]) -> Coverage {
        Coverage::SeasonPack {
            seasons: seasons.to_vec(),
            complete: false,
        }
    }

    #[test]
    fn parses_real_release_names() {
        let cases = [
            (
                "Dune.2021.1080p.BluRay.x264-GRP",
                "dune",
                Some(2021),
                Coverage::Movie,
            ),
            (
                "The.Matrix.1999.2160p.UHD.BluRay.REMUX",
                "matrix",
                Some(1999),
                Coverage::Movie,
            ),
            (
                "Blade Runner 2049 (2017) 1080p WEB-DL",
                "blade runner 2049",
                Some(2017),
                Coverage::Movie,
            ),
            (
                "Game.of.Thrones.S01E02.720p.HDTV.x264",
                "game of thrones",
                None,
                episodes(1, &[2]),
            ),
            (
                "Show.Name.S02E03E04.1080p.WEB",
                "show name",
                None,
                episodes(2, &[3, 4]),
            ),
            (
                "Show.Name.S02E03-E05.1080p.WEB",
                "show name",
                None,
                episodes(2, &[3, 4, 5]),
            ),
            ("Show Name 3x07 HDTV", "show name", None, episodes(3, &[7])),
            (
                "Show.Name.S01.1080p.BluRay.x265",
                "show name",
                None,
                pack(&[1]),
            ),
            (
                "Show Name Season 2 Complete 720p",
                "show name",
                None,
                Coverage::SeasonPack {
                    seasons: vec![2],
                    complete: true,
                },
            ),
            (
                "Show.Name.S01-S03.1080p",
                "show name",
                None,
                pack(&[1, 2, 3]),
            ),
            (
                "Show Name Complete Series 1080p",
                "show name",
                None,
                Coverage::SeasonPack {
                    seasons: vec![],
                    complete: true,
                },
            ),
            (
                "Doctor.Who.2005.S10E01.720p",
                "doctor who",
                Some(2005),
                episodes(10, &[1]),
            ),
            ("Amélie.2001.1080p", "amelie", Some(2001), Coverage::Movie),
            (
                "Marvel's.Agents.of.S.H.I.E.L.D.S01E01",
                "marvels agents of s h i e l d",
                None,
                episodes(1, &[1]),
            ),
        ];
        for (name, title, year, coverage) in cases {
            let parsed = parse(name);
            assert_eq!(parsed.title, title, "title of {name}");
            assert_eq!(parsed.year, year, "year of {name}");
            assert_eq!(parsed.coverage, coverage, "coverage of {name}");
        }
    }

    #[test]
    fn normalizes_titles_for_matching() {
        assert_eq!(
            normalize_title("The Lord of the Rings"),
            "lord of the rings"
        );
        assert_eq!(normalize_title("Law & Order: SVU"), "law and order svu");
        assert_eq!(normalize_title("Don't Look Up"), "dont look up");
        assert_eq!(normalize_title("The"), "the");
    }

    #[test]
    fn matches_titles_and_aliases() {
        let titles = vec![
            "Blade Runner 2049".to_string(),
            "Dune: Part One".to_string(),
        ];
        assert!(title_matches(
            &parse("Blade.Runner.2049.1080p.WEB"),
            &titles
        ));
        assert!(title_matches(&parse("Dune.Part.One.2021.2160p"), &titles));
        assert!(!title_matches(&parse("Dune.Part.Two.2024.2160p"), &titles));
        assert!(!title_matches(&parse("Blade.Runner.1982.1080p"), &titles));
    }

    #[test]
    fn knows_which_seasons_are_covered() {
        assert!(pack(&[1, 2]).covers_season(2));
        assert!(episodes(3, &[1]).covers_season(3));
        assert!(!Coverage::Movie.covers_season(1));
        assert!(
            Coverage::SeasonPack {
                seasons: vec![],
                complete: true
            }
            .covers_season(9)
        );
    }
}
