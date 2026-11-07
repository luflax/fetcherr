# fetcherr

A lean replacement for the parts of Radarr/Sonarr I use: search releases through **Prowlarr**,
download them with **qBittorrent**, and follow progress live. Rust backend with a REST API
(Swagger), a WebSocket and an htmx web UI, packaged as a Home Assistant add-on.

Architecture and coding guidelines: [AGENTS.md](AGENTS.md).

## Install in Home Assistant

1. **Settings → Add-ons → Add-on Store → ⋮ → Repositories**
2. Add: `https://github.com/luflax/fetcherr#shipping`
3. Install **Fetcherr** and start it. Open it from the sidebar and configure it under **Settings**.

> HACS cannot install add-ons; use the Add-on Store repository above.

## Development

```sh
cd fetcherr
cargo test
cargo run            # http://localhost:8734 (PORT=...), settings in ./data (DATA_DIR=...)
```

- UI: <http://localhost:8734/>
- Swagger: <http://localhost:8734/docs/>
- Live feed: `ws://localhost:8734/api/ws/downloads`

For a local stack, run `lscr.io/linuxserver/prowlarr` (port 9696) and
`lscr.io/linuxserver/qbittorrent` (port 8080) in Docker and point the settings at them.

Docker:

```sh
docker build -t fetcherr:dev fetcherr/
docker run --rm -p 8734:8734 -v fetcherr-data:/data fetcherr:dev
```

## Release flow

- Work happens on `develop`. Releases ship by PR from `develop` → `shipping`.
- Every push to `shipping` triggers `.github/workflows/build.yaml`, which builds
  `ghcr.io/luflax/fetcherr-{amd64,aarch64}` on native runners and pushes `:<version>` and `:latest`.
- **Before merging to `shipping`:** bump `version` in `fetcherr/config.yaml` (and
  `fetcherr/Cargo.toml`), and add a `fetcherr/CHANGELOG.md` entry. Home Assistant only offers an
  update when the version changes, and pulls the image tagged with that version.
- After the very first build, set both GHCR packages (`fetcherr-amd64`, `fetcherr-aarch64`)
  to **Public** in GitHub → Packages → Package settings, so Home Assistant can pull them.
