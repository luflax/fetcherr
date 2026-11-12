# Fetcherr

Fetcherr lets you browse movies and series (from **TMDB**), request them, and follow them until
they are in **Jellyfin**. It finds releases through **Prowlarr**, downloads them with
**qBittorrent**, renames them Jellyfin-style and puts them in your library. You can also
search and download releases by hand. Use it from the web UI (the **Fetcherr** entry in the Home
Assistant sidebar) or from its REST API.

## Getting started

Open **Fetcherr** from the sidebar and go to **Settings**:

1. **Indexer (Prowlarr):** the URL (e.g. `http://192.168.1.10:9696`) and the API key
   (Prowlarr → Settings → General). Click **Test**, then **Save**.
2. **Download client (qBittorrent):** the WebUI URL, username, password and the default
   download folder, the path *as qBittorrent sees it* (e.g. `/downloads`).
3. **Metadata (TMDB):** a TMDB API key or read access token (themoviedb.org → Settings → API),
   plus the language (e.g. `en-US`) and, optionally, a region for release dates.
4. **Media server (Jellyfin):** the URL and an API key (Jellyfin → Dashboard → API Keys).
   Fetcherr reads the library every few minutes (15 by default) to know what you already have.
5. **Ratings (OMDb):** optional. With an OMDb key, titles also show IMDb, Rotten Tomatoes and
   Metacritic scores.
6. **Library:** the movies and series folders *as Fetcherr sees them* (e.g. `/media/movies`),
   how to import (hardlink or move), and **path mappings** when qBittorrent and Fetcherr see the
   download folder under different paths (e.g. `/downloads = /share/downloads`).
7. **Media types:** for **Movies**, **Series** and **Software**, the indexer categories, the
   download folder and, for Movies and Series, the allowed qualities in order of preference.
8. **Discover:** the pages (tabs) and lists that client apps show. See *Discover layout*.

The add-on can read and write `/media` and `/share`, so put the library (and, for hardlinks,
the downloads) there.

## Requests

A request is a movie, a whole season, or some episodes of a season. When you create one:

1. Fetcherr works out what is missing. For a season, that is every **aired** episode that is
   not already in Jellyfin, imported, or being downloaded for another request.
2. It searches Prowlarr for the title, drops releases of other titles (titles are compared with
   the TMDB title, original title and aliases; movies also by year) and tells season packs from
   single episodes.
3. It picks the best release by your quality ranking, then seeders. For a season with nothing
   yet, it takes a season pack. For a partly available season, it takes single episodes for
   what is missing, and a pack only for episodes nobody released on their own; from a pack it
   imports only the missing episodes. You can also pick a release yourself (`release_id`).
4. It follows the download. When it finishes, the files are imported: the main video (and
   subtitles) is **hardlinked** into the library (a copy when the folders are on different
   disks; the torrent keeps seeding) or **moved** (the torrent is then removed). Names follow
   Jellyfin's conventions, e.g.
   `Movies/Dune (2021) [tmdbid-438631]/Dune (2021).mkv` and
   `Series/Game of Thrones (2011) [tmdbid-1399]/Season 01/Game of Thrones S01E02.mkv`.
   The templates can be changed under **Library → Naming**.
5. Fetcherr asks Jellyfin to rescan, and the request becomes **available**.

Request states: `searching`, `not_found` (searched again every 6 hours), `downloading`,
`paused`, `importing`, `available`, `failed`. Requests are stored in `/data/fetcherr.db`.

Downloads are still the qBittorrent torrents tagged `fetcherr`, so they also show under
**Downloads**.

## Discover layout

**Settings → Discover** sets the pages (tabs) and, for each page, its ordered lists. A list
shows titles from a TMDB source: trending (today or this week), popular, top rated, in
theatres, upcoming, on the air, airing today, or **discover** with filters (genre ids, year
range, minimum vote, sort order, original language, watch providers and region). Use the
arrows to reorder, then **Save layout**.

## Access and the API key

Every `/api` endpoint, the WebSockets included, needs the **API key** shown under
**Settings → API key**: send it in the `X-Api-Key` header, or as the `api_key` query parameter
(needed for WebSockets from a browser). `/health`, the API docs and the web UI pages need no
key. **Generate a new key** there if it leaks; the old one stops working at once.

- **Sidebar (Ingress):** protected by your Home Assistant login.
- **Port 8734:** the same UI and API on your LAN (e.g. `http://homeassistant.local:8734`).
  The API needs the key, but the web UI itself (which can show the key) has no login. Remove
  the port mapping in the add-on's *Network* section if you only want Ingress.

## REST API

Interactive docs (Swagger UI): `http://homeassistant.local:8734/docs/`. The raw OpenAPI spec is
at `/docs/openapi.json`; it documents every endpoint, model and the WebSocket event format. The
web UI uses this same API for everything it shows and does.

Lists are paginated the same way everywhere, 20 items per page:
`{"page": 1, "total_pages": 12, "total_results": 236, "items": [...]}`.

Movies and series appear as one **card** model everywhere:

```json
{"key": "movie:438631", "media_type": "movie", "tmdb_id": 438631, "title": "Dune",
 "year": 2021, "poster": {"small": "…/w185/….jpg", "medium": "…/w342/….jpg",
 "large": "…/w500/….jpg", "original": "…/original/….jpg"}, "backdrop": {…},
 "overview": "…", "vote_average": 7.8, "availability": "available",
 "jellyfin_item_id": "4f2a1c3b…"}
```

`availability` is `available`, `partially_available`, `downloading`, `requested` or
`unavailable`.

| Method | Path | Purpose |
|--------|------|---------|
| GET | `/api/system/status` | Version and which services are configured |
| POST | `/api/library/sync` | Read the Jellyfin library now |
| GET | `/api/discover/pages` | Ordered tabs (`ETag`/`Cache-Control`) |
| GET | `/api/discover/pages/{id}` | A page's ordered lists, each with its first page of cards (`ETag`) |
| GET | `/api/discover/lists/{id}?page=` | One list, page by page (`ETag`) |
| GET | `/api/movies/{tmdb_id}` | Details, availability, requests, cast and related lists |
| GET | `/api/series/{tmdb_id}` | Details and seasons with availability |
| GET | `/api/series/{tmdb_id}/seasons/{n}` | Episodes with availability and request state |
| GET | `/api/{movies\|series}/{tmdb_id}/ratings` | TMDB, IMDb, Rotten Tomatoes and Metacritic scores |
| GET | `/api/movies/{tmdb_id}/releases` | Releases of a movie, for picking one by hand |
| GET | `/api/series/{tmdb_id}/releases?season=&episode=` | Releases of a series, season or episode |
| GET | `/api/people/{tmdb_id}` | Biography, photo and credits |
| GET | `/api/titles/search?query=&page=` | Movies, series and people |
| POST | `/api/requests` | `{"media_type", "tmdb_id", "scope", "quality"?, "release_id"?}` → `202`, or `200` with the existing request |
| GET | `/api/requests?state=&page=` | Requests, newest first |
| GET | `/api/requests/{id}` | A request with its torrents and imported files |
| DELETE | `/api/requests/{id}?delete_files=` | Remove a request and its torrents (and the files, with `true`) → `204` |
| POST | `/api/requests/{id}/pause` / `resume` / `retry` | Request controls |
| GET/PUT | `/api/settings/tmdb`, `/omdb`, `/jellyfin`, `/library`, `/layout` | Settings (secrets are write-only) |
| POST | `/api/settings/tmdb/test`, `/jellyfin/test` | Test unsaved settings |
| GET | `/api/settings/api-key` | The API key |
| POST | `/api/settings/api-key/regenerate` | Replace the API key |
| GET/PUT | `/api/settings/indexer` | Prowlarr settings (`api_key` is write-only) |
| POST | `/api/settings/indexer/test` | Test unsaved Prowlarr settings; returns version + categories |
| GET/PUT | `/api/settings/download-client` | qBittorrent settings (`password` is write-only) |
| POST | `/api/settings/download-client/test` | Test unsaved qBittorrent settings |
| GET | `/api/settings/media-types` | Settings of every media type |
| PUT | `/api/settings/media-types/{media_type}` | `movies`/`series`/`software`: `{"categories", "download_folder", "allowed_qualities"}` |
| GET | `/api/indexer/categories` | Indexer categories |
| GET | `/api/qualities` | Every quality id and label |
| GET | `/api/search` | Release search: `query`, `media_type`, `categories`, `qualities`, `indexer_ids`, `min_seeders`, `min_size`, `max_size`, `sort`, `limit` |
| POST | `/api/downloads` | Start a download: `{"release_id", "media_type"?, "save_path"?}` → `202 {"id": "<info-hash>"}` |
| GET | `/api/downloads`, `/api/downloads/{id}` | Fetcherr downloads |
| POST | `/api/downloads/{id}/pause` / `resume` | Pause or resume a download → `204` |
| DELETE | `/api/downloads/{id}?delete_files=false` | Remove a download → `204` |
| GET | `/health` | Liveness (no key needed) |

A request `scope` is `{"type": "movie"}`, `{"type": "season", "season": 1}` or
`{"type": "episodes", "season": 1, "episodes": [3, 4]}`.

Layout and list responses carry an `ETag` and `Cache-Control: private, max-age=60`; send the
ETag back in `If-None-Match` to get `304 Not Modified` when nothing changed.

Errors always look like `{"error": {"code": "...", "message": "..."}}`:

| Status | Code | Meaning |
|--------|------|---------|
| 401 | `unauthorized` | Missing or wrong API key |
| 404 | `not_found` | Unknown page, list, request, title or download |
| 409 | `not_configured` | A service is not configured yet |
| 409 | `conflict` | Not possible in the current state (e.g. pausing a request that is not downloading) |
| 410 | `release_expired` | The `release_id` is older than 1 hour; search again |
| 422 | `invalid` | Invalid input |
| 502 | `upstream` | Prowlarr, qBittorrent, TMDB, Jellyfin or OMDb failed (the message says why) |

`release_id` values come from searches. Fetcherr keeps each release's download link on the
server, so indexer API keys never reach clients.

### Example

```sh
KEY=…   # Settings → API key
curl -H "X-Api-Key: $KEY" "http://homeassistant.local:8734/api/discover/pages/home"
curl -H "X-Api-Key: $KEY" -X POST http://homeassistant.local:8734/api/requests \
     -H 'content-type: application/json' \
     -d '{"media_type": "series", "tmdb_id": 1399, "scope": {"type": "season", "season": 1}}'
```

## Live events (WebSocket)

`GET /api/ws/events?api_key=<key>` streams request progress and availability changes as JSON
text messages. Narrow it with `requests=<id>,<id>` and/or `titles=movie:603,series:1399`;
without either you get everything.

```json
{"type": "request_updated", "request": {"id": 7, "key": "series:1399", "state": "downloading", "progress": 0.12, …}}
{"type": "request_progress", "request_id": 7, "key": "series:1399", "progress": 0.42, "dl_speed": 5242880, "eta_secs": 487}
{"type": "request_deleted", "request_id": 7, "key": "series:1399"}
{"type": "availability_changed", "key": "series:1399", "availability": "available", "jellyfin_item_id": "c3d4…", "season": 1, "episode": 3}
```

- On connect (and if a client falls behind) Fetcherr sends a `request_updated` for every
  matching request (every unfinished one without a filter), then events as they happen.
- `availability_changed` without `season`/`episode` is about the whole title. For a series it
  says `partially_available` when the series is in Jellyfin; fetch the series for the exact
  state.
- Progress comes from the same single qBittorrent poll as the downloads feed below, which only
  runs while something downloads or someone watches.

### Download progress

`GET /api/ws/downloads?api_key=<key>` (optionally `&ids=<id>,<id>`) streams the raw downloads:

```json
{"type": "update", "downloads": [{"id": "d12c…", "name": "…", "state": "downloading",
  "progress": 0.42, "size": 4402341478, "downloaded": 1849000000, "dl_speed": 5242880,
  "ul_speed": 0, "eta_secs": 487, "seeds": 9, "peers": 2, "save_path": "/downloads",
  "media_type": "movies"}]}
{"type": "removed", "ids": ["d12c…"]}
```

- The first message is a full snapshot. After that, each `update` carries only the downloads
  that changed, and `removed` lists downloads that disappeared.
- `state` is one of `queued`, `fetching_metadata`, `downloading`, `stalled`, `paused`,
  `checking`, `moving`, `seeding`, `completed`, `error` or `unknown`.
- Fetcherr polls qBittorrent once per *Progress refresh* interval (default 1000 ms).

## Media types and qualities

Each release gets a media type: the one you searched, or the one whose categories match the
release (sub-categories count, e.g. 2040 *Movies/HD* belongs to 2000 *Movies*).

Movies and Series releases get a quality, detected from the title like Radarr does:
Remux-2160p, Bluray-2160p, WEB-2160p, Remux-1080p, Bluray-1080p, WEB-1080p, HDTV-1080p,
Bluray-720p, WEB-720p, HDTV-720p, DVD, SDTV, CAM/TS or Unknown. Searches only include the
media type's allowed qualities (none ticked allows all) and are ranked by your quality order
first, then by seeders, size or age. A request can ask for one specific quality instead.

## Options

| Option      | Default | Description                                       |
|-------------|---------|---------------------------------------------------|
| `log_level` | `info`  | One of `trace`, `debug`, `info`, `warn`, `error`. |
