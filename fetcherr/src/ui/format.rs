//! Human-readable values for templates (`downloads.js` mirrors these for live updates).

const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];

pub fn bytes(value: u64) -> String {
    let mut size = value as f64;
    let mut unit = 0;
    while size >= 1024.0 && unit < UNITS.len() - 1 {
        size /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{value} B")
    } else {
        format!("{size:.1} {}", UNITS[unit])
    }
}

pub fn speed(bytes_per_sec: u64) -> String {
    if bytes_per_sec == 0 {
        "—".into()
    } else {
        format!("{}/s", bytes(bytes_per_sec))
    }
}

pub fn eta(seconds: Option<u64>) -> String {
    let Some(seconds) = seconds else {
        return "∞".into();
    };
    match seconds {
        0 => "—".into(),
        s if s < 60 => format!("{s}s"),
        s if s < 3600 => format!("{}m {}s", s / 60, s % 60),
        s if s < 86_400 => format!("{}h {}m", s / 3600, (s % 3600) / 60),
        s => format!("{}d {}h", s / 86_400, (s % 86_400) / 3600),
    }
}

pub fn age(hours: f64) -> String {
    match hours {
        h if h < 1.0 => format!("{} min", (h * 60.0).round() as u64),
        h if h < 48.0 => format!("{} h", h.round() as u64),
        h if h < 24.0 * 60.0 => format!("{} days", (h / 24.0).round() as u64),
        h => format!("{} months", (h / 24.0 / 30.0).round() as u64),
    }
}

pub fn percent(fraction: f64) -> String {
    format!("{:.1}", fraction * 100.0)
}

pub fn gigabytes_to_bytes(gigabytes: f64) -> u64 {
    (gigabytes * 1024.0 * 1024.0 * 1024.0).round() as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_sizes_and_durations() {
        assert_eq!(bytes(512), "512 B");
        assert_eq!(bytes(1536), "1.5 KB");
        assert_eq!(bytes(3 * 1024 * 1024 * 1024), "3.0 GB");
        assert_eq!(eta(Some(3725)), "1h 2m");
        assert_eq!(eta(None), "∞");
        assert_eq!(age(72.0), "3 days");
        assert_eq!(percent(0.4567), "45.7");
    }
}
