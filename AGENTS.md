# AGENTS.md

Fetcherr replaces Radarr/Sonarr for one user. It searches releases through Prowlarr, downloads
them with qBittorrent and streams progress live. It is a Rust (axum) service shipped as a
Home Assistant add-on (`fetcherr/`). Read this file before changing code.

## Architecture

```
browser ──htmx──▶ ui/ ──ApiClient (in-process HTTP)──▶ api/ ──▶ services ──▶ providers ──▶ Prowlarr / qBittorrent
browser ──WebSocket────────────────────────────────▶ api/ws ◀── downloads::monitor (one poller, watch channel)
```

| Layer | Modules | Owns |
|-------|---------|------|
| Presentation | `api/` (JSON + OpenAPI), `ui/` (askama + htmx) | parsing input, rendering output |
| Services | `search`, `downloads`, `settings`, `integrations` | business rules |
| Providers | `prowlarr`, `qbittorrent` (+ private `dto.rs`) | talking to upstreams |
| Domain | `domain`, `quality`, `torrent`, `releases` | shared types, quality detection, info-hash, release cache |

- **No database.** Settings are stored in `<data dir>/settings.json`, written atomically.
  Downloads are the qBittorrent torrents tagged `fetcherr`.
- **Download id = torrent v1 info-hash**, resolved before the torrent reaches qBittorrent.
- **Release ids are opaque.** Download links (which contain indexer API keys) stay in the
  server-side `ReleaseCache`.
- **Live progress:** `DownloadMonitor` polls qBittorrent at a fixed rate, only while someone is
  watching. It publishes snapshots on a `watch` channel, and `DownloadFeed` sends each viewer
  only what changed.
- **Settings hot-reload:** saving settings rebuilds `Integrations` and swaps it in. No restart.

## Rules

- **The UI only talks to the REST API.** It uses `ui::ApiClient` server-side or `fetch`/WebSocket
  in the browser, and never calls services or providers. A new UI feature needs an API endpoint
  first.
- **Upstream docs are the source of truth:** `docs/upstream/`. Write DTOs from those files, and
  never guess. When reality differs from the docs, handle both and note it next to the code
  (e.g. qBittorrent 5.3 returns `401` on a bad login).
- Handlers stay thin: extract input, make one service call, then respond. Business logic belongs in services.
- Provider DTOs never leave their module; map them to `domain` types.
- One shared `reqwest::Client` (`integrations::http_client`). Never create a client per request.
- No blocking calls on async tasks; use `tokio::fs`.
- Never poll upstreams once per viewer; live data flows through the monitor's `watch` channel.
- Secrets are write-only: views expose `has_api_key` / `has_password`, and an empty input keeps
  the stored value.
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
