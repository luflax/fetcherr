//! Wall-clock helpers: Unix seconds for storage, ISO dates for comparing TMDB air dates.

use std::time::{SystemTime, UNIX_EPOCH};

const SECS_PER_DAY: i64 = 86_400;

pub fn now_unix() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_secs() as i64)
}

/// Today's UTC date as `YYYY-MM-DD`, comparable as a string with TMDB dates.
pub fn today() -> String {
    let (year, month, day) = civil_from_days(now_unix().div_euclid(SECS_PER_DAY));
    format!("{year:04}-{month:02}-{day:02}")
}

/// RFC 3339 UTC timestamp, e.g. `2026-11-12T08:30:00Z`.
pub fn rfc3339(unix_secs: i64) -> String {
    let days = unix_secs.div_euclid(SECS_PER_DAY);
    let secs_of_day = unix_secs.rem_euclid(SECS_PER_DAY);
    let (year, month, day) = civil_from_days(days);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
        secs_of_day / 3600,
        secs_of_day % 3600 / 60,
        secs_of_day % 60
    )
}

/// Proleptic Gregorian date of a day count since 1970-01-01 (Howard Hinnant's algorithm).
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let day_of_era = z.rem_euclid(146_097);
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_index = (5 * day_of_year + 2) / 153;
    let day = (day_of_year - (153 * month_index + 2) / 5 + 1) as u32;
    let month = if month_index < 10 {
        month_index + 3
    } else {
        month_index - 9
    } as u32;
    let year = year_of_era + era * 400 + i64::from(month <= 2);
    (year, month, day)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_unix_seconds_as_rfc3339() {
        assert_eq!(rfc3339(0), "1970-01-01T00:00:00Z");
        assert_eq!(rfc3339(951_782_400), "2000-02-29T00:00:00Z");
        assert_eq!(rfc3339(1_794_472_245), "2026-11-12T08:30:45Z");
    }

    #[test]
    fn today_is_an_iso_date() {
        let today = today();
        assert_eq!(today.len(), 10);
        assert!(today.as_str() > "2026-01-01");
    }
}
