//! Release search: indexer query, media type and quality rules, local filters and sorting,
//! then caching of download sources.

use std::cmp::Reverse;

use crate::domain::{MediaType, Release, SearchQuery, SortBy};
use crate::error::AppError;
use crate::integrations::Integrations;
use crate::prowlarr::SearchRequest;
use crate::quality::Quality;
use crate::releases::{CachedRelease, ReleaseCache};
use crate::settings::MediaSettings;

pub async fn search(
    integrations: &Integrations,
    releases: &ReleaseCache,
    media: &MediaSettings,
    query: &SearchQuery,
) -> Result<Vec<Release>, AppError> {
    let categories = categories_to_search(media, query);
    let found = integrations
        .prowlarr()?
        .search(&SearchRequest {
            query: query.query.trim(),
            categories: &categories,
            indexer_ids: &query.indexer_ids,
            limit: query.limit,
        })
        .await?;

    let found: Vec<_> = found
        .into_iter()
        .map(|mut found| {
            found.release.media_type = query
                .media_type
                .or_else(|| media_type_of(&found.release.categories, media));
            found
        })
        .collect();
    releases.remember(found.iter().map(|found| {
        let cached = CachedRelease {
            source: found.source.clone(),
            media_type: found.release.media_type,
        };
        (found.release.release_id.clone(), cached)
    }));

    let mut matching: Vec<Release> = found
        .into_iter()
        .map(|found| found.release)
        .filter(|release| passes_filters(query, media, release))
        .collect();
    sort_releases(&mut matching, query, media);
    Ok(matching)
}

/// Explicit categories, else the media type's, else those of every media type.
fn categories_to_search(media: &MediaSettings, query: &SearchQuery) -> Vec<u32> {
    if !query.categories.is_empty() {
        return query.categories.clone();
    }
    let media_types = match query.media_type {
        Some(media_type) => vec![media_type],
        None => MediaType::ALL.to_vec(),
    };
    let mut categories: Vec<u32> = media_types
        .into_iter()
        .flat_map(|media_type| media.get(media_type).categories.iter().copied())
        .collect();
    categories.sort_unstable();
    categories.dedup();
    categories
}

/// The first media type with a category matching the release (or its parent, e.g. 2040 → 2000).
fn media_type_of(categories: &[u32], media: &MediaSettings) -> Option<MediaType> {
    MediaType::ALL.into_iter().find(|media_type| {
        let configured = &media.get(*media_type).categories;
        categories.iter().any(|category| {
            configured.contains(category) || configured.contains(&(category / 1000 * 1000))
        })
    })
}

fn passes_filters(query: &SearchQuery, media: &MediaSettings, release: &Release) -> bool {
    let enough_seeders = query.min_seeders.is_none_or(|min| release.seeders >= min);
    let above_min_size = query.min_size.is_none_or(|min| release.size >= min);
    let below_max_size = query.max_size.is_none_or(|max| release.size <= max);
    let quality_allowed = quality_preferences(query, media, release)
        .is_none_or(|ranked| ranked.is_empty() || ranked.contains(&release.quality));
    enough_seeders && above_min_size && below_max_size && quality_allowed
}

/// The qualities that apply to a release, most preferred first: the requested ones, else its
/// media type's. `None` when qualities don't apply (Software, or an unknown media type).
fn quality_preferences<'a>(
    query: &'a SearchQuery,
    media: &'a MediaSettings,
    release: &Release,
) -> Option<&'a [Quality]> {
    match release.media_type {
        Some(media_type) if !media_type.has_qualities() => None,
        _ if !query.qualities.is_empty() => Some(&query.qualities),
        Some(media_type) => Some(&media.get(media_type).allowed_qualities),
        None => None,
    }
}

/// Position in the quality preferences; unranked releases come last.
fn quality_rank(query: &SearchQuery, media: &MediaSettings, release: &Release) -> usize {
    quality_preferences(query, media, release)
        .and_then(|ranked| {
            ranked
                .iter()
                .position(|quality| *quality == release.quality)
        })
        .unwrap_or(usize::MAX)
}

/// Quality preference first, then the requested order (stable, so ties keep indexer order).
fn sort_releases(releases: &mut [Release], query: &SearchQuery, media: &MediaSettings) {
    match query.sort {
        SortBy::Seeders => releases.sort_by_key(|r| Reverse(r.seeders)),
        SortBy::Size => releases.sort_by_key(|r| Reverse(r.size)),
        SortBy::Age => releases.sort_by(|a, b| a.age_hours.total_cmp(&b.age_hours)),
    }
    releases.sort_by_key(|release| quality_rank(query, media, release));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::ReleaseId;

    fn release(seeders: u32, size: u64, age_hours: f64) -> Release {
        Release {
            release_id: ReleaseId::from_guid(1, &format!("{seeders}-{size}")),
            title: "t".into(),
            indexer: "i".into(),
            indexer_id: 1,
            size,
            seeders,
            leechers: 0,
            published_at: String::new(),
            age_hours,
            categories: vec![],
            info_url: None,
            quality: Quality::Unknown,
            media_type: None,
        }
    }

    fn movie(quality: Quality, seeders: u32) -> Release {
        Release {
            quality,
            media_type: Some(MediaType::Movies),
            ..release(seeders, 1, 0.0)
        }
    }

    fn media_with_movie_qualities(ranked: Vec<Quality>) -> MediaSettings {
        let mut media = MediaSettings::default();
        media.movies.allowed_qualities = ranked;
        media
    }

    fn sorted(mut releases: Vec<Release>, query: &SearchQuery, media: &MediaSettings) -> Vec<u32> {
        sort_releases(&mut releases, query, media);
        releases.iter().map(|release| release.seeders).collect()
    }

    #[test]
    fn filters_by_seeders_and_size() {
        let query = SearchQuery {
            min_seeders: Some(5),
            min_size: Some(100),
            max_size: Some(1000),
            ..Default::default()
        };
        let media = MediaSettings::default();
        assert!(passes_filters(&query, &media, &release(5, 500, 0.0)));
        assert!(!passes_filters(&query, &media, &release(4, 500, 0.0)));
        assert!(!passes_filters(&query, &media, &release(9, 50, 0.0)));
        assert!(!passes_filters(&query, &media, &release(9, 5000, 0.0)));
    }

    #[test]
    fn searches_the_media_type_categories_or_all_of_them() {
        let media = MediaSettings::default();
        let movies = SearchQuery {
            media_type: Some(MediaType::Movies),
            ..Default::default()
        };
        assert_eq!(categories_to_search(&media, &movies), vec![2000]);
        assert_eq!(
            categories_to_search(&media, &SearchQuery::default()),
            vec![2000, 4000, 5000]
        );
        let explicit = SearchQuery {
            categories: vec![2040],
            ..movies
        };
        assert_eq!(categories_to_search(&media, &explicit), vec![2040]);
    }

    #[test]
    fn infers_media_type_from_sub_categories() {
        let media = MediaSettings::default();
        assert_eq!(media_type_of(&[5040], &media), Some(MediaType::Series));
        assert_eq!(media_type_of(&[4000], &media), Some(MediaType::Software));
        assert_eq!(media_type_of(&[3000], &media), None);
    }

    #[test]
    fn keeps_only_allowed_qualities() {
        let media = media_with_movie_qualities(vec![Quality::Web1080p]);
        let query = SearchQuery::default();
        assert!(passes_filters(&query, &media, &movie(Quality::Web1080p, 1)));
        assert!(!passes_filters(&query, &media, &movie(Quality::Cam, 1)));

        let requested = SearchQuery {
            qualities: vec![Quality::Cam],
            ..Default::default()
        };
        assert!(passes_filters(&requested, &media, &movie(Quality::Cam, 1)));
        assert!(!passes_filters(
            &requested,
            &media,
            &movie(Quality::Web1080p, 1)
        ));
    }

    #[test]
    fn software_ignores_qualities() {
        let query = SearchQuery {
            qualities: vec![Quality::Web1080p],
            ..Default::default()
        };
        let software = Release {
            media_type: Some(MediaType::Software),
            ..release(1, 1, 0.0)
        };
        assert!(passes_filters(&query, &MediaSettings::default(), &software));
    }

    #[test]
    fn quality_preference_order_sorts_before_seeders() {
        let media = media_with_movie_qualities(vec![Quality::Bluray1080p, Quality::Web2160p]);
        let releases = vec![
            movie(Quality::Web2160p, 50),
            movie(Quality::Bluray1080p, 3),
            movie(Quality::Web2160p, 90),
            movie(Quality::Bluray1080p, 7),
        ];
        assert_eq!(
            sorted(releases, &SearchQuery::default(), &media),
            vec![7, 3, 90, 50]
        );
    }

    #[test]
    fn requested_qualities_rank_in_the_order_given() {
        let media = media_with_movie_qualities(vec![Quality::Bluray1080p, Quality::Web2160p]);
        let query = SearchQuery {
            qualities: vec![Quality::Web2160p, Quality::Bluray1080p],
            ..Default::default()
        };
        let releases = vec![movie(Quality::Bluray1080p, 9), movie(Quality::Web2160p, 1)];
        assert_eq!(sorted(releases, &query, &media), vec![1, 9]);
    }

    #[test]
    fn without_preferences_the_chosen_sort_decides() {
        let releases = vec![movie(Quality::Cam, 1), movie(Quality::Remux2160p, 9)];
        let query = SearchQuery::default();
        assert_eq!(
            sorted(releases, &query, &MediaSettings::default()),
            vec![9, 1]
        );
    }

    #[test]
    fn sorts_newest_first_by_age() {
        let query = SearchQuery {
            sort: SortBy::Age,
            ..Default::default()
        };
        let releases = vec![release(1, 1, 30.0), release(2, 2, 1.0)];
        assert_eq!(
            sorted(releases, &query, &MediaSettings::default()),
            vec![2, 1]
        );
    }

    #[test]
    fn sorts_most_seeded_first() {
        let releases = vec![release(1, 1, 0.0), release(9, 2, 0.0)];
        let query = SearchQuery::default();
        assert_eq!(
            sorted(releases, &query, &MediaSettings::default()),
            vec![9, 1]
        );
    }
}
