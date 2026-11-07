//! Release quality (source + resolution), detected from the release title like Radarr does.

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// Ordered best to worst.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, ToSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum Quality {
    #[serde(rename = "remux_2160p")]
    Remux2160p,
    #[serde(rename = "bluray_2160p")]
    Bluray2160p,
    #[serde(rename = "web_2160p")]
    Web2160p,
    #[serde(rename = "remux_1080p")]
    Remux1080p,
    #[serde(rename = "bluray_1080p")]
    Bluray1080p,
    #[serde(rename = "web_1080p")]
    Web1080p,
    #[serde(rename = "hdtv_1080p")]
    Hdtv1080p,
    #[serde(rename = "bluray_720p")]
    Bluray720p,
    #[serde(rename = "web_720p")]
    Web720p,
    #[serde(rename = "hdtv_720p")]
    Hdtv720p,
    Dvd,
    Sdtv,
    Cam,
    Unknown,
}

impl Quality {
    pub const ALL: [Quality; 14] = [
        Self::Remux2160p,
        Self::Bluray2160p,
        Self::Web2160p,
        Self::Remux1080p,
        Self::Bluray1080p,
        Self::Web1080p,
        Self::Hdtv1080p,
        Self::Bluray720p,
        Self::Web720p,
        Self::Hdtv720p,
        Self::Dvd,
        Self::Sdtv,
        Self::Cam,
        Self::Unknown,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Remux2160p => "Remux-2160p",
            Self::Bluray2160p => "Bluray-2160p",
            Self::Web2160p => "WEB-2160p",
            Self::Remux1080p => "Remux-1080p",
            Self::Bluray1080p => "Bluray-1080p",
            Self::Web1080p => "WEB-1080p",
            Self::Hdtv1080p => "HDTV-1080p",
            Self::Bluray720p => "Bluray-720p",
            Self::Web720p => "WEB-720p",
            Self::Hdtv720p => "HDTV-720p",
            Self::Dvd => "DVD",
            Self::Sdtv => "SDTV",
            Self::Cam => "CAM/TS",
            Self::Unknown => "Unknown",
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|quality| quality.id() == id)
    }

    /// Stable id used in query strings and forms (same as the JSON value).
    pub fn id(self) -> &'static str {
        match self {
            Self::Remux2160p => "remux_2160p",
            Self::Bluray2160p => "bluray_2160p",
            Self::Web2160p => "web_2160p",
            Self::Remux1080p => "remux_1080p",
            Self::Bluray1080p => "bluray_1080p",
            Self::Web1080p => "web_1080p",
            Self::Hdtv1080p => "hdtv_1080p",
            Self::Bluray720p => "bluray_720p",
            Self::Web720p => "web_720p",
            Self::Hdtv720p => "hdtv_720p",
            Self::Dvd => "dvd",
            Self::Sdtv => "sdtv",
            Self::Cam => "cam",
            Self::Unknown => "unknown",
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Resolution {
    R2160,
    R1080,
    R720,
    Sd,
}

#[derive(Clone, Copy, PartialEq)]
enum Source {
    Remux,
    Bluray,
    Web,
    Hdtv,
    Dvd,
    Sdtv,
    Cam,
}

pub fn parse(title: &str) -> Quality {
    let title = title.to_ascii_lowercase();
    let tokens: Vec<&str> = title
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|token| !token.is_empty())
        .collect();
    let resolution = tokens.iter().find_map(|token| resolution(token));
    let source = best_source(&tokens);
    combine(source, resolution)
}

fn resolution(token: &str) -> Option<Resolution> {
    match token {
        "2160p" | "4k" | "uhd" => Some(Resolution::R2160),
        "1080p" | "1080i" => Some(Resolution::R1080),
        "720p" => Some(Resolution::R720),
        "480p" | "576p" => Some(Resolution::Sd),
        _ => None,
    }
}

fn source(token: &str) -> Option<Source> {
    match token {
        "remux" => Some(Source::Remux),
        "bluray" | "bdrip" | "brrip" | "bd" => Some(Source::Bluray),
        "web" | "webdl" | "webrip" | "amzn" | "nf" | "dsnp" => Some(Source::Web),
        "hdtv" => Some(Source::Hdtv),
        "dvd" | "dvdrip" | "dvdr" => Some(Source::Dvd),
        "sdtv" | "pdtv" | "dsr" => Some(Source::Sdtv),
        "cam" | "hdcam" | "ts" | "telesync" | "hdts" | "tc" => Some(Source::Cam),
        _ => None,
    }
}

/// Titles often name several sources ("BluRay.REMUX", "WEB-DL" split into "web"/"dl");
/// the most specific one wins.
fn best_source(tokens: &[&str]) -> Option<Source> {
    let found: Vec<Source> = tokens.iter().filter_map(|token| source(token)).collect();
    let pairs = tokens.windows(2).filter_map(|pair| match pair {
        ["blu", "ray"] => Some(Source::Bluray),
        ["web", "dl"] => Some(Source::Web),
        _ => None,
    });
    let found: Vec<Source> = found.into_iter().chain(pairs).collect();
    [
        Source::Cam,
        Source::Remux,
        Source::Bluray,
        Source::Web,
        Source::Hdtv,
        Source::Dvd,
        Source::Sdtv,
    ]
    .into_iter()
    .find(|candidate| found.contains(candidate))
}

fn combine(source: Option<Source>, resolution: Option<Resolution>) -> Quality {
    use Quality::*;
    match (source, resolution) {
        (Some(Source::Cam), _) => Cam,
        (Some(Source::Remux), Some(Resolution::R2160)) => Remux2160p,
        (Some(Source::Bluray), Some(Resolution::R2160)) => Bluray2160p,
        (_, Some(Resolution::R2160)) => Web2160p,
        (Some(Source::Remux), Some(Resolution::R1080)) => Remux1080p,
        (Some(Source::Bluray), Some(Resolution::R1080)) => Bluray1080p,
        (Some(Source::Web), Some(Resolution::R1080)) => Web1080p,
        (_, Some(Resolution::R1080)) => Hdtv1080p,
        (Some(Source::Remux | Source::Bluray), Some(Resolution::R720)) => Bluray720p,
        (Some(Source::Web), Some(Resolution::R720)) => Web720p,
        (_, Some(Resolution::R720)) => Hdtv720p,
        (Some(Source::Dvd), _) => Dvd,
        (Some(Source::Sdtv | Source::Hdtv | Source::Web), Some(Resolution::Sd) | None) => Sdtv,
        (Some(Source::Bluray), Some(Resolution::Sd)) => Dvd,
        _ => Unknown,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_source_and_resolution() {
        let cases = [
            (
                "Dune.Part.Two.2024.2160p.UHD.BluRay.REMUX.HDR.HEVC.Atmos-FGT",
                Quality::Remux2160p,
            ),
            (
                "Dune Part Two 2024 2160p UHD BluRay x265-TERMiNAL",
                Quality::Bluray2160p,
            ),
            (
                "Dune.Part.Two.2024.4K.WEB-DL.DDP5.1.H.265-NTb",
                Quality::Web2160p,
            ),
            (
                "Oppenheimer.2023.1080p.BluRay.REMUX.AVC.DTS-HD.MA.5.1-FGT",
                Quality::Remux1080p,
            ),
            (
                "Oppenheimer (2023) 1080p Blu-ray x264",
                Quality::Bluray1080p,
            ),
            (
                "The.Bear.S03E01.1080p.AMZN.WEB-DL.DDP5.1.H.264-NTb",
                Quality::Web1080p,
            ),
            ("The.Bear.S03E01.1080p.WEBRip.x264-ION10", Quality::Web1080p),
            (
                "Some.Show.S01E01.1080i.HDTV.MPA2.0.H.264",
                Quality::Hdtv1080p,
            ),
            ("Movie.2010.720p.BRRip.x264", Quality::Bluray720p),
            ("Show.S02E05.720p.NF.WEB-DL", Quality::Web720p),
            ("Show.S02E05.720p.HDTV.x264-KILLERS", Quality::Hdtv720p),
            ("Old.Movie.1999.DVDRip.XviD", Quality::Dvd),
            ("Show.S01E01.PDTV.XviD", Quality::Sdtv),
            ("Show.S01E01.480p.WEB.h264", Quality::Sdtv),
            ("New.Movie.2024.HDCAM.x264", Quality::Cam),
            ("New.Movie.2024.1080p.TELESYNC", Quality::Cam),
            ("debian-12.5.0-amd64-netinst.iso", Quality::Unknown),
        ];
        for (title, expected) in cases {
            assert_eq!(parse(title), expected, "{title}");
        }
    }

    #[test]
    fn resolution_without_source_uses_radarr_defaults() {
        assert_eq!(parse("Movie 2024 2160p x265"), Quality::Web2160p);
        assert_eq!(parse("Movie 2024 1080p x264"), Quality::Hdtv1080p);
        assert_eq!(parse("Movie 2024 720p x264"), Quality::Hdtv720p);
    }

    #[test]
    fn json_value_matches_id() {
        for quality in Quality::ALL {
            let json = serde_json::to_string(&quality).unwrap();
            assert_eq!(json, format!("\"{}\"", quality.id()));
        }
    }
}
