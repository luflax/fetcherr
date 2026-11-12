# Upstream API documentation

Snapshots of the APIs Fetcherr integrates with. **Treat them as the source of truth** when
writing or changing the provider DTOs (`fetcherr/src/*/dto.rs`).

| File | Source | Fetched |
|------|--------|---------|
| `prowlarr-openapi.json` | <https://raw.githubusercontent.com/Prowlarr/Prowlarr/develop/src/Prowlarr.Api.V1/openapi.json> | 2026-11-07 |
| `qbittorrent-webui-api-5.0.md` | <https://github.com/qbittorrent/qBittorrent/wiki/WebUI-API-(qBittorrent-5.0)> (raw wiki markdown) | 2026-11-07 |
| `tmdb-v3-openapi.json` | <https://developer.themoviedb.org/openapi/tmdb-api.json> | 2026-11-12 |
| `jellyfin-openapi-stable.json` | <https://api.jellyfin.org/openapi/jellyfin-openapi-stable.json> (Jellyfin 12.4.0) | 2026-11-12 |
| `omdb-swagger.json` | <https://www.omdbapi.com/swagger.json> | 2026-11-12 |

Refresh them when upgrading a provider:

```sh
curl -L https://raw.githubusercontent.com/Prowlarr/Prowlarr/develop/src/Prowlarr.Api.V1/openapi.json \
     -o docs/upstream/prowlarr-openapi.json
curl -L "https://raw.githubusercontent.com/wiki/qbittorrent/qBittorrent/WebUI-API-(qBittorrent-5.0).md" \
     -o docs/upstream/qbittorrent-webui-api-5.0.md
```

## Known differences between the docs and real servers

The code handles these explicitly:

- **Prowlarr `magnetUrl`** is usually a Prowlarr proxy URL (`…/download?apikey=…&link=…`) that
  redirects to the `magnet:` link, not the magnet itself.
- **qBittorrent 5.3** answers a bad login with `401 Unauthorized`. The docs only describe
  `200` with the body `Fails.`.
- **qBittorrent 5.x** reports `stoppedDL`/`stoppedUP`. The docs list `pausedDL`/`pausedUP`.
- **qBittorrent 5.3** answers a successful login with `204` and no body. The session cookie
  can be named `QBT_SID_<port>`. `torrents/add` returns JSON (`added_torrent_ids`, …) instead of
  `Ok.`. Only `Fails.` counts as a failure.
- **qBittorrent `torrents/add` with a `category`** creates that category when it doesn't exist
  (checked on 5.3). The docs don't say either way.
- **qBittorrent `torrents/stop|start|delete`** return `200` for any hash. Fetcherr checks the
  torrent is one of its own first, so it can answer `404`.
- **OMDb's swagger** documents the parameters but not the response. The DTO follows OMDb's
  documented example (`Ratings[{Source, Value}]`, `imdbRating`, `imdbVotes`, `Metascore`, with
  `"N/A"` for missing values). Failures answer HTTP 200 with `{"Response": "False", "Error": …}`
  (401 for a bad key).
- **TMDB** answers an unknown id with `404` and `status_code: 34`. Its documented example for
  `recommendations` is an empty object; the DTO treats every list as optional.
- **TMDB alternative titles** are under `titles` for movies and `results` for series.
- **Jellyfin** episodes carry no TMDB id: they are matched to their series by `SeriesId`.
  `fixtures/jellyfin-items.json` follows the `BaseItemDto` schema; it is not a capture from a
  real server yet.
