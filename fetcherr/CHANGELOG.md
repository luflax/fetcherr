# Changelog

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
