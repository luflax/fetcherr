# Fetcherr

Fetcherr finds releases through **Prowlarr** and sends them to **qBittorrent**. Use it
from the web UI (the **Fetcherr** entry in the Home Assistant sidebar) or from its REST API.

## Getting started

1. Open **Fetcherr** from the sidebar and go to **Settings**.
2. **Indexer (Prowlarr):** enter the URL (e.g. `http://192.168.1.10:9696`) and the API key
   (Prowlarr → Settings → General). Click **Test**, then **Save**.
3. **Download client (qBittorrent):** enter the WebUI URL, username, password and the default
   download folder, the path *as qBittorrent sees it* (e.g. `/downloads`). Click **Test**, then
   **Save**.
4. **Media types:** for **Movies**, **Series** and **Software**, pick the indexer categories to
   search, the download folder (empty uses the default one) and, for Movies and Series, the
   allowed qualities in order of preference (use the arrows to rank them).
5. **Search**, choose a media type (or *All*), pick a release and click **Download**. Follow it
   live under **Downloads**, where you can also pause, resume or delete it.

Fetcherr tags its torrents with `fetcherr` in qBittorrent, and **Downloads** lists only
those torrents. Each download also gets the qBittorrent category of its media type (`movies`,
`series` or `software`), created on first use, and is added with Automatic Torrent Management
off, so the media type's folder is always used. It has no database: settings live in
`/data/settings.json`.

## Media types and qualities

Each release gets a media type: the one you searched, or the one whose categories match the
release (sub-categories count, e.g. 2040 *Movies/HD* belongs to 2000 *Movies*).

Movies and Series releases get a quality, detected from the title like Radarr does:
Remux-2160p, Bluray-2160p, WEB-2160p, Remux-1080p, Bluray-1080p, WEB-1080p, HDTV-1080p,
Bluray-720p, WEB-720p, HDTV-720p, DVD, SDTV, CAM/TS or Unknown. Search results only include the
media type's allowed qualities (none ticked allows all) and are ranked by your quality order
first, then by the chosen sort (seeders, size or age). Software has no qualities.

## Access

- **Sidebar (Ingress):** protected by your Home Assistant login.
- **Port 8734:** the same UI and API on your LAN (e.g. `http://homeassistant.local:8734`).
  This port has **no authentication**. Remove the port mapping in the add-on's *Network*
  section if you only want Ingress.

## REST API

Interactive docs (Swagger UI): `http://homeassistant.local:8734/docs/`. The raw OpenAPI spec is
at `/docs/openapi.json`. The web UI uses this same API for everything it shows and does.

| Method | Path | Purpose |
|--------|------|---------|
| GET/PUT | `/api/settings/indexer` | Prowlarr settings (`api_key` is write-only) |
| POST | `/api/settings/indexer/test` | Test unsaved Prowlarr settings; returns version + categories |
| GET/PUT | `/api/settings/download-client` | qBittorrent settings (`password` is write-only) |
| POST | `/api/settings/download-client/test` | Test unsaved qBittorrent settings |
| GET | `/api/settings/media-types` | Settings of every media type |
| PUT | `/api/settings/media-types/{media_type}` | `movies`/`series`/`software`: `{"categories", "download_folder", "allowed_qualities"}`, qualities most preferred first |
| GET | `/api/indexer/categories` | Indexer categories |
| GET | `/api/qualities` | Every quality id and label |
| GET | `/api/search` | Search: `query`, `media_type`, `categories`, `qualities`, `indexer_ids`, `min_seeders`, `min_size`, `max_size` (bytes), `sort` (`seeders`/`size`/`age`), `limit`. Repeat a key for several values; `qualities` are ranked in the order given |
| POST | `/api/downloads` | Start a download: `{"release_id": "...", "media_type": "optional", "save_path": "optional"}` → `202 {"id": "<info-hash>"}` |
| GET | `/api/downloads` | All Fetcherr downloads |
| GET | `/api/downloads/{id}` | One download |
| POST | `/api/downloads/{id}/pause` | Pause a download → `204` |
| POST | `/api/downloads/{id}/resume` | Resume a download → `204` |
| DELETE | `/api/downloads/{id}?delete_files=false` | Remove a download from qBittorrent (and its files with `delete_files=true`) → `204` |
| GET | `/health` | Liveness (used by the watchdog) |

Errors always look like `{"error": {"code": "...", "message": "..."}}`:

| Status | Code | Meaning |
|--------|------|---------|
| 409 | `not_configured` | Prowlarr or qBittorrent is not configured yet |
| 410 | `release_expired` | The `release_id` is older than 1 hour; search again |
| 422 | `invalid` | Invalid input |
| 502 | `upstream` | Prowlarr or qBittorrent failed (the message says why) |

`release_id` values come from search results. Fetcherr keeps each release's download link on
the server, so indexer API keys never reach clients.

### Example

```sh
curl "http://homeassistant.local:8734/api/search?query=dune&media_type=movies&qualities=bluray_1080p&min_seeders=5"
curl -X POST http://homeassistant.local:8734/api/downloads \
     -H 'content-type: application/json' -d '{"release_id":"a90721d80345c77475b0"}'
```

## Live progress (WebSocket)

`GET /api/ws/downloads` (optionally `?ids=<id>,<id>` to follow specific downloads) streams
JSON messages:

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
- Fetcherr polls qBittorrent once per *Progress refresh* interval (default 1000 ms, set in
  Settings), and only while at least one client is connected. Any number of viewers share
  that single poll.

## Options

| Option      | Default | Description                                       |
|-------------|---------|---------------------------------------------------|
| `log_level` | `info`  | One of `trace`, `debug`, `info`, `warn`, `error`. |
