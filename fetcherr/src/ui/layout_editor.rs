//! The discover layout editor on the Settings page. The browser edits the rows and saves the
//! whole layout with `PUT api/settings/layout` (see `static/layout-editor.js`).

use askama::Template;

use crate::settings::layout::{DiscoverFilters, DiscoverPage, ListDefinition, TimeWindow};
use crate::settings::{Layout, ListSource};
use crate::titles::TitleKind;

const KINDS: [(&str, &str); 8] = [
    ("trending", "Trending"),
    ("popular", "Popular"),
    ("top_rated", "Top rated"),
    ("now_playing", "In theatres (movies)"),
    ("upcoming", "Upcoming (movies)"),
    ("on_the_air", "On the air (series)"),
    ("airing_today", "Airing today (series)"),
    ("discover", "Discover (with filters)"),
];
const MEDIAS: [(&str, &str); 3] = [
    ("", "Movies and series"),
    ("movie", "Movies"),
    ("series", "Series"),
];
const WINDOWS: [(&str, &str); 2] = [("week", "This week"), ("day", "Today")];

pub struct SelectOption {
    pub value: &'static str,
    pub label: &'static str,
    pub selected: bool,
}

fn options(choices: &[(&'static str, &'static str)], selected: &str) -> Vec<SelectOption> {
    choices
        .iter()
        .map(|(value, label)| SelectOption {
            value,
            label,
            selected: *value == selected,
        })
        .collect()
}

#[derive(Template)]
#[template(path = "partials/layout_editor.html")]
pub struct LayoutEditor {
    pages: Vec<PageRow>,
    blank_page: PageRow,
    blank_list: ListRow,
}

impl LayoutEditor {
    pub fn new(layout: &Layout) -> Self {
        Self {
            pages: layout.pages.iter().map(PageRow::new).collect(),
            blank_page: PageRow {
                id: String::new(),
                name: String::new(),
                lists: Vec::new(),
            },
            blank_list: ListRow::new(&ListDefinition {
                id: String::new(),
                title: String::new(),
                source: ListSource::Popular {
                    media: TitleKind::Movie,
                },
                filters: DiscoverFilters::default(),
            }),
        }
    }
}

#[derive(Template)]
#[template(path = "partials/layout_page.html")]
struct PageRow {
    id: String,
    name: String,
    lists: Vec<ListRow>,
}

impl PageRow {
    fn new(page: &DiscoverPage) -> Self {
        Self {
            id: page.id.clone(),
            name: page.name.clone(),
            lists: page.lists.iter().map(ListRow::new).collect(),
        }
    }
}

#[derive(Template)]
#[template(path = "partials/layout_list.html")]
struct ListRow {
    id: String,
    title: String,
    kinds: Vec<SelectOption>,
    medias: Vec<SelectOption>,
    windows: Vec<SelectOption>,
    genres: String,
    year_from: String,
    year_to: String,
    min_vote: String,
    min_votes: String,
    sort_by: String,
    original_language: String,
    watch_providers: String,
    watch_region: String,
}

impl ListRow {
    fn new(list: &ListDefinition) -> Self {
        let (kind, window) = match list.source {
            ListSource::Trending { window, .. } => ("trending", window),
            ListSource::Popular { .. } => ("popular", TimeWindow::Week),
            ListSource::TopRated { .. } => ("top_rated", TimeWindow::Week),
            ListSource::NowPlaying => ("now_playing", TimeWindow::Week),
            ListSource::Upcoming => ("upcoming", TimeWindow::Week),
            ListSource::OnTheAir => ("on_the_air", TimeWindow::Week),
            ListSource::AiringToday => ("airing_today", TimeWindow::Week),
            ListSource::Discover { .. } => ("discover", TimeWindow::Week),
        };
        let media = list.source.media().map_or("", TitleKind::id);
        let filters = &list.filters;
        let text = |value: Option<String>| value.unwrap_or_default();
        let joined = |ids: &[u32]| {
            ids.iter()
                .map(u32::to_string)
                .collect::<Vec<_>>()
                .join(", ")
        };
        Self {
            id: list.id.clone(),
            title: list.title.clone(),
            kinds: options(&KINDS, kind),
            medias: options(&MEDIAS, media),
            windows: options(&WINDOWS, window.id()),
            genres: joined(&filters.genres),
            year_from: text(filters.year_from.map(|year| year.to_string())),
            year_to: text(filters.year_to.map(|year| year.to_string())),
            min_vote: text(filters.min_vote.map(|vote| vote.to_string())),
            min_votes: text(filters.min_votes.map(|votes| votes.to_string())),
            sort_by: text(filters.sort_by.clone()),
            original_language: text(filters.original_language.clone()),
            watch_providers: joined(&filters.watch_providers),
            watch_region: text(filters.watch_region.clone()),
        }
    }
}
