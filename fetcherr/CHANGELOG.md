# Changelog

## 0.4.0

- **Pair TVs with a code:** a device calls `POST /api/auth/pair`, shows the code, and polls
  `GET /api/auth/pair/{secret}` (both without a key) until the code is entered under
  **Settings → Devices**; it then gets its own API key. Devices are listed there and can be
  removed, which revokes their key.
- **API key:** every `/api` endpoint and the WebSockets now need the API key (Settings → API
  key), in the `X-Api-Key` header or the `api_key` query parameter. `/health`, the API docs and
  the web UI stay open. Existing clients must add the key.
- **Browse with TMDB:** configurable discover pages and lists (trending, popular, top rated, in
  theatres, upcoming, on the air, airing today, or discover with filters), title and person
  details, search and sized image URLs. One card model and one pagination shape
  (`{page, total_pages, total_results, items}`, 20 per page) everywhere. Layout and list
  responses carry `ETag` and `Cache-Control`.
- **Jellyfin:** availability and Jellyfin item ids per title, season and episode, synced every
  few minutes and after each import.
- **Requests:** request a movie, a season or episodes. Fetcherr searches by title (dropping other
  titles, telling season packs from single episodes), picks the best release by your quality
  ranking, and for a partly available season grabs only the missing episodes. Pause, resume,
  retry and delete requests. Requests are stored in `/data/fetcherr.db`. A request needs the
  library folder for its kind; retrying after a failed import imports the finished download
  again instead of downloading it again.
- **Import:** finished downloads are hardlinked (or moved) into the library with Jellyfin-style
  names, with subtitles, then Jellyfin rescans. Path mappings cover qBittorrent and Fetcherr
  seeing different folders. The add-on can now read and write `/media` and `/share`.
- **Ratings:** IMDb, Rotten Tomatoes and Metacritic through OMDb, cached for a day.
- **Live events:** `GET /api/ws/events` pushes request progress and availability changes,
  filtered by request or title. The format is in the OpenAPI spec (`Event`).
- `GET /api/system/status` reports the version and which services are configured.
- Settings for TMDB, OMDb, Jellyfin, the library, the discover layout and the API key, in the
  API and the web UI.

## 0.3.0

- Media types: **Movies**, **Series** and **Software**, each with its own indexer categories and
  download folder. Downloads get the matching qBittorrent category.
- Qualities for Movies and Series, detected from release titles (Remux, Bluray, WEB and HDTV
  at 2160p, 1080p and 720p, plus DVD, SDTV and CAM/TS). Each type has a ranked list of allowed
  qualities.
- Search by media type and quality. Results only include allowed qualities and are sorted by
  your quality ranking first, then by seeders, size or age.
- Pause, resume and delete downloads from the Downloads page or the API. When you delete a
  download, you can also delete its files.
- Gear icon for Settings; the top bar shows just the name.
- The indexer's "default categories" setting is replaced by the media types' categories.

## 0.2.0

- Search releases through Prowlarr, with filters for categories, seeders and size, and sorting.
- Start downloads in qBittorrent. The download id is the torrent info-hash, known up front.
- Live download progress over a WebSocket, using one shared poll of qBittorrent (1 s by
  default, configurable).
- Web UI (search, downloads, settings) available in the Home Assistant sidebar via Ingress.
- Settings with **Test** buttons for Prowlarr and qBittorrent, stored in `/data/settings.json`.
- REST API documented with OpenAPI / Swagger UI at `/docs/`.
- Removed the `/api/hello` example endpoint.

## 0.1.1

- Listen on IPv6 and IPv4 (dual-stack) so the API is reachable via `homeassistant.local`,
  which resolves to IPv6 first.

## 0.1.0

- Initial release: `GET /api/hello` and `GET /health`.
