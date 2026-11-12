# AGENTS.md

Fetcherr replaces Radarr/Sonarr (and Jellyseerr) for one user. It browses TMDB, takes requests,
searches releases through Prowlarr, downloads them with qBittorrent, imports them into the
Jellyfin library and streams progress live. It is a Rust (axum) service shipped as a Home
Assistant add-on (`fetcherr/`). Read this file before changing code.

## Architecture

```
browser ──htmx──▶ ui/ ──ApiClient (in-process HTTP)──▶ api/ (API key) ──▶ services ──▶ providers ──▶ upstreams
client app / browser ──REST + WebSocket────────────────▶ api/ ◀── events (broadcast) ◀── requests, library
requests::worker ──▶ search ──▶ downloads ──▶ downloads::monitor (one poller, watch channel) ──▶ import
library ──▶ Jellyfin (sync loop, watch channel)                     catalog ──▶ TMDB (+ OMDb ratings)
```

| Layer | Modules | Owns |
|-------|---------|------|
| Presentation | `api/` (JSON + OpenAPI, `auth`, `http_cache`), `ui/` (askama + htmx) | parsing input, rendering output |
| Services | `catalog`, `requests` (+ `worker`), `import`, `library`, `search`, `downloads`, `settings`, `integrations`, `events` | business rules |
| Providers | `prowlarr`, `qbittorrent`, `tmdb`, `jellyfin`, `omdb` (+ private `dto.rs`) | talking to upstreams |
| Domain | `domain`, `titles`, `metadata`, `release_name`, `quality`, `torrent`, `releases`, `cache`, `clock` | shared types and pure rules |
| Storage | `db` (SQLite via sqlx, `migrations/`), `settings::store` (`settings.json`) | persistence |

- **Storage:** settings live in `<data dir>/settings.json`, written atomically. Requests, their
  torrents, imported files and the OMDb cache live in SQLite (`<data dir>/fetcherr.db`);
  migrations in `fetcherr/migrations/` are embedded at compile time and run at startup. Never
  edit an applied migration; add a new one.
- **Downloads** are the qBittorrent torrents tagged `fetcherr`. **Download id = torrent v1
  info-hash**, resolved before the torrent reaches qBittorrent.
- **Release ids are opaque.** Download links (which contain indexer API keys) stay in the
  server-side `ReleaseCache`.
- **Titles** are keyed `movie:<tmdb_id>` / `series:<tmdb_id>` (`titles::TitleKey`). Every list
  returns `titles::Card` and every paginated response is `domain::Page` (20 per page).
- **Live data:** `DownloadMonitor` polls qBittorrent only while someone watches (a viewer or
  the request tracker) and publishes snapshots on a `watch` channel. Request and availability
  events go through `events::EventBus`; each WebSocket filters them.
- **Settings hot-reload:** saving settings rebuilds `Integrations` and swaps it in. No restart.

## Rules

- **Every `/api` route needs the API key** (`api::auth`), except what the in-process
  `ApiClient` sends (marked with the `InternalCall` extension). Only `/health`, `/docs` and UI
  pages are open.
- **The UI only talks to the REST API.** It uses `ui::ApiClient` server-side or `fetch`/WebSocket
  in the browser, and never calls services or providers. A new UI feature needs an API endpoint
  first.
- **Upstream docs are the source of truth:** `docs/upstream/`. Write DTOs from those files, and
  never guess. When reality differs from the docs, handle both and note it next to the code
  (e.g. qBittorrent 5.3 returns `401` on a bad login).
- Handlers stay thin: extract input, make one service call, then respond. Business logic belongs in services.
- Provider DTOs never leave their module; map them to Fetcherr types (`domain`, `metadata`,
  `library`).
- One shared `reqwest::Client` (`integrations::http_client`). Never create a client per request.
- No blocking calls on async tasks; use `tokio::fs`.
- Never poll upstreams once per viewer; live data flows through the monitor's `watch` channel
  and the event bus.
- SQL lives in the service that owns the table (`requests/store.rs`, `catalog/ratings.rs`),
  as static query strings. Use `tokio::fs` for every file operation, imports included.
- Secrets are write-only: views expose `has_api_key` / `has_password`, and an empty input keeps
  the stored value. The one exception is Fetcherr's own API key (`/api/settings/api-key`), which
  only callers holding it can read.
- Every UI URL is relative (`<base href>` comes from `X-Ingress-Path`), so it works behind
  HA Ingress.
- Every API endpoint gets `#[utoipa::path]` and is registered in `api::router`. Error bodies use
  `AppError`.
- Third-party JS is vendored in `static/`. The add-on must work offline.

## Code style

- Prefer readable code over comments. Use small functions, descriptive names and types that make
  invalid states unrepresentable. Refactor before you annotate; comments explain *why*, never *what*.
- Each module owns its types. Errors use `thiserror` and are mapped to HTTP in `error.rs`.
- No `unwrap`/`expect` outside tests.
- Test business rules with unit tests, and providers with `wiremock`. Test flows end to end in
  `src/tests.rs` (UI → API → service → mocked upstream). Fixtures come from real upstream
  payloads (`tests/fixtures/`) with the secrets scrubbed.

## Workflow

```sh
cd fetcherr
cargo fmt && cargo clippy --all-targets -- -D warnings && cargo test
cargo run   # http://localhost:8734, data in ./data
```

- Port 8734. In dev, override with `PORT` and `DATA_DIR`.
- Before a PR from `develop` to `shipping`: bump `version` in `fetcherr/config.yaml` and
  `Cargo.toml`, and add a `CHANGELOG.md` entry.
