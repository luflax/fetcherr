# Changelog

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
