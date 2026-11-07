# Upstream API documentation

Snapshots of the APIs Fetcherr integrates with. **Treat them as the source of truth** when
writing or changing the provider DTOs (`fetcherr/src/*/dto.rs`).

| File | Source | Fetched |
|------|--------|---------|
| `prowlarr-openapi.json` | <https://raw.githubusercontent.com/Prowlarr/Prowlarr/develop/src/Prowlarr.Api.V1/openapi.json> | 2026-11-07 |
| `qbittorrent-webui-api-5.0.md` | <https://github.com/qbittorrent/qBittorrent/wiki/WebUI-API-(qBittorrent-5.0)> (raw wiki markdown) | 2026-11-07 |

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
