//! Release search: indexer query, local filters and sorting, then caching of download sources.

use std::cmp::Reverse;

use crate::domain::{Release, SearchQuery, SortBy};
use crate::error::AppError;
use crate::integrations::Integrations;
use crate::prowlarr::SearchRequest;
use crate::releases::ReleaseCache;

pub async fn search(
    integrations: &Integrations,
    releases: &ReleaseCache,
    default_categories: &[u32],
    query: &SearchQuery,
) -> Result<Vec<Release>, AppError> {
    let categories = if query.categories.is_empty() {
        default_categories
    } else {
        &query.categories
    };
    let found = integrations
        .prowlarr()?
        .search(&SearchRequest {
            query: query.query.trim(),
            categories,
            indexer_ids: &query.indexer_ids,
            limit: query.limit,
        })
        .await?;

    releases.remember(
        found
            .iter()
            .map(|found| (found.release.release_id.clone(), found.source.clone())),
    );

    let mut matching: Vec<Release> = found
        .into_iter()
        .map(|found| found.release)
        .filter(|release| passes_filters(query, release))
        .collect();
    sort_releases(&mut matching, query.sort);
    Ok(matching)
}

fn passes_filters(query: &SearchQuery, release: &Release) -> bool {
    let enough_seeders = query.min_seeders.is_none_or(|min| release.seeders >= min);
    let above_min_size = query.min_size.is_none_or(|min| release.size >= min);
    let below_max_size = query.max_size.is_none_or(|max| release.size <= max);
    enough_seeders && above_min_size && below_max_size
}

fn sort_releases(releases: &mut [Release], sort: SortBy) {
    match sort {
        SortBy::Seeders => releases.sort_by_key(|r| Reverse(r.seeders)),
        SortBy::Size => releases.sort_by_key(|r| Reverse(r.size)),
        SortBy::Age => releases.sort_by(|a, b| a.age_hours.total_cmp(&b.age_hours)),
    }
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
        }
    }

    #[test]
    fn filters_by_seeders_and_size() {
        let query = SearchQuery {
            min_seeders: Some(5),
            min_size: Some(100),
            max_size: Some(1000),
            ..Default::default()
        };
        assert!(passes_filters(&query, &release(5, 500, 0.0)));
        assert!(!passes_filters(&query, &release(4, 500, 0.0)));
        assert!(!passes_filters(&query, &release(9, 50, 0.0)));
        assert!(!passes_filters(&query, &release(9, 5000, 0.0)));
    }

    #[test]
    fn sorts_newest_first_by_age() {
        let mut releases = vec![release(1, 1, 30.0), release(2, 2, 1.0)];
        sort_releases(&mut releases, SortBy::Age);
        assert_eq!(releases[0].seeders, 2);
    }

    #[test]
    fn sorts_most_seeded_first() {
        let mut releases = vec![release(1, 1, 0.0), release(9, 2, 0.0)];
        sort_releases(&mut releases, SortBy::Seeders);
        assert_eq!(releases[0].seeders, 9);
    }
}
