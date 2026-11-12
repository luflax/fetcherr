//! OMDb payloads. `docs/upstream/omdb-swagger.json` documents the parameters but not the
//! response, so these fields follow OMDb's documented example (see `docs/upstream/README.md`).

use serde::Deserialize;

/// `GET /?i=<imdb id>` (only the fields Fetcherr reads). Values are strings; `"N/A"` when missing.
#[derive(Debug, Deserialize)]
pub struct TitleResponse {
    /// `"True"` or `"False"`; failures still answer HTTP 200, with `Error`.
    #[serde(rename = "Response")]
    pub response: Option<String>,
    #[serde(rename = "Error")]
    pub error: Option<String>,
    #[serde(rename = "Ratings", default)]
    pub ratings: Vec<Rating>,
    #[serde(rename = "imdbRating")]
    pub imdb_rating: Option<String>,
    #[serde(rename = "imdbVotes")]
    pub imdb_votes: Option<String>,
    #[serde(rename = "Metascore")]
    pub metascore: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct Rating {
    #[serde(rename = "Source")]
    pub source: String,
    #[serde(rename = "Value")]
    pub value: String,
}
