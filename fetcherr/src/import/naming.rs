//! Library names from templates such as `{title} ({year}) [tmdbid-{tmdb_id}]`, made safe for
//! every filesystem.

const PLACEHOLDERS: [&str; 6] = ["title", "year", "tmdb_id", "season", "episode", "quality"];

/// Values a template can use.
#[derive(Debug, Clone, Default)]
pub struct NameValues<'a> {
    pub title: &'a str,
    pub year: Option<i32>,
    pub tmdb_id: u32,
    pub season: Option<u32>,
    /// Several for a multi-episode file.
    pub episodes: &'a [u32],
    pub quality: &'a str,
}

/// Checks that a template is non-empty and only uses known placeholders.
pub fn check_template(template: &str, needs_episode: bool) -> Result<(), String> {
    if template.trim().is_empty() {
        return Err("must not be empty".into());
    }
    let mut uses_episode = false;
    for placeholder in placeholders(template)? {
        let (name, format) = placeholder.split_once(':').unwrap_or((placeholder, ""));
        if !PLACEHOLDERS.contains(&name) {
            return Err(format!("unknown placeholder {{{name}}}"));
        }
        if !format.is_empty() && !is_padding(format) {
            return Err(format!("unknown format '{format}' in {{{placeholder}}}"));
        }
        uses_episode |= name == "episode";
    }
    if needs_episode && !uses_episode {
        return Err("must contain {episode}".into());
    }
    Ok(())
}

/// Renders a template into one safe path component.
pub fn render(template: &str, values: &NameValues<'_>) -> String {
    let mut rendered = String::with_capacity(template.len() + values.title.len());
    let mut rest = template;
    while let Some(start) = rest.find('{') {
        rendered.push_str(&rest[..start]);
        let Some(length) = rest[start..].find('}') else {
            rest = &rest[start..];
            break;
        };
        let placeholder = &rest[start + 1..start + length];
        rendered.push_str(&value_of(placeholder, values));
        rest = &rest[start + length + 1..];
    }
    rendered.push_str(rest);
    sanitize(&rendered)
}

fn placeholders(template: &str) -> Result<Vec<&str>, String> {
    let mut found = Vec::new();
    let mut rest = template;
    while let Some(start) = rest.find('{') {
        let end = rest[start..]
            .find('}')
            .ok_or_else(|| "has an unclosed '{'".to_string())?;
        found.push(&rest[start + 1..start + end]);
        rest = &rest[start + end + 1..];
    }
    Ok(found)
}

/// `02` pads numbers to two digits.
fn is_padding(format: &str) -> bool {
    format.len() == 2 && format.starts_with('0') && format[1..].parse::<usize>().is_ok()
}

fn value_of(placeholder: &str, values: &NameValues<'_>) -> String {
    let (name, format) = placeholder.split_once(':').unwrap_or((placeholder, ""));
    let width = if is_padding(format) {
        format[1..].parse().unwrap_or(0)
    } else {
        0
    };
    let number = |n: u32| format!("{n:0width$}");
    match name {
        "title" => values.title.to_string(),
        "year" => values.year.map(|year| year.to_string()).unwrap_or_default(),
        "tmdb_id" => values.tmdb_id.to_string(),
        "season" => values.season.map(number).unwrap_or_default(),
        "episode" => match values.episodes {
            [] => String::new(),
            [single] => number(*single),
            [first, .., last] => format!("{}-E{}", number(*first), number(*last)),
        },
        "quality" => values.quality.to_string(),
        _ => String::new(),
    }
}

/// Removes characters that Windows, macOS or Linux reject in names, and the leftovers of
/// empty placeholders such as `()` when a title has no year.
pub fn sanitize(name: &str) -> String {
    let mut cleaned = String::with_capacity(name.len());
    for c in name.chars() {
        match c {
            ':' => cleaned.push_str(" -"),
            '<' | '>' | '"' | '/' | '\\' | '|' | '?' | '*' => {}
            c if c.is_control() => {}
            c => cleaned.push(c),
        }
    }
    let cleaned = cleaned.replace("()", "").replace("[]", "");
    let cleaned = cleaned.split_whitespace().collect::<Vec<_>>().join(" ");
    let cleaned = cleaned.trim_end_matches(['.', ' ']).trim_start_matches('.');
    if cleaned.is_empty() {
        "Unknown".into()
    } else {
        cleaned.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::LibrarySettings;

    #[test]
    fn renders_jellyfin_style_names() {
        let naming = LibrarySettings::default().naming;
        let movie = NameValues {
            title: "Mission: Impossible",
            year: Some(1996),
            tmdb_id: 954,
            ..Default::default()
        };
        assert_eq!(
            render(&naming.movie_folder, &movie),
            "Mission - Impossible (1996) [tmdbid-954]"
        );
        assert_eq!(
            render(&naming.movie_file, &movie),
            "Mission - Impossible (1996)"
        );

        let episode = NameValues {
            title: "Game of Thrones",
            year: Some(2011),
            tmdb_id: 1399,
            season: Some(1),
            episodes: &[2],
            ..Default::default()
        };
        assert_eq!(render(&naming.season_folder, &episode), "Season 01");
        assert_eq!(
            render(&naming.episode_file, &episode),
            "Game of Thrones S01E02"
        );
        let double = NameValues {
            episodes: &[1, 2],
            ..episode
        };
        assert_eq!(
            render(&naming.episode_file, &double),
            "Game of Thrones S01E01-E02"
        );
    }

    #[test]
    fn drops_empty_year_and_unsafe_characters() {
        let values = NameValues {
            title: "What? / Why*",
            tmdb_id: 1,
            ..Default::default()
        };
        assert_eq!(render("{title} ({year})", &values), "What Why");
        assert_eq!(sanitize("..."), "Unknown");
    }

    #[test]
    fn checks_templates() {
        assert!(check_template("{title} S{season:02}E{episode:02}", true).is_ok());
        assert!(check_template("{title}", true).is_err());
        assert!(check_template("{name}", false).is_err());
        assert!(check_template("{title", false).is_err());
        assert!(check_template("{season:x2}", false).is_err());
    }
}
