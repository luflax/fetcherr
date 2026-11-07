# fetcherr

Rust backend API packaged as a Home Assistant add-on. It currently serves a hello-world
endpoint and will grow into a backend that talks to external APIs and other HA add-ons.

## Install in Home Assistant

1. **Settings → Add-ons → Add-on Store → ⋮ → Repositories**
2. Add: `https://github.com/luflax/fetcherr#shipping`
3. Install **Fetcherr**, start it, then open `http://<ha-host>:8734/api/hello`.

> HACS cannot install add-ons; use the Add-on Store repository above.

## Development

```sh
cd fetcherr
cargo test
cargo run            # listens on 0.0.0.0:8734 (override with PORT=...)
curl localhost:8734/api/hello
```

Docker:

```sh
docker build -t fetcherr:dev fetcherr/
docker run --rm -p 8734:8734 fetcherr:dev
```

## Release flow

- Work happens on `develop`. Releases ship by PR from `develop` → `shipping`.
- Every push to `shipping` triggers `.github/workflows/build.yaml`, which builds
  `ghcr.io/luflax/fetcherr-{amd64,aarch64}` on native runners and pushes `:<version>` and `:latest`.
- **Before merging to `shipping`:** bump `version` in `fetcherr/config.yaml` and add a
  `fetcherr/CHANGELOG.md` entry. Home Assistant only offers an update when the version changes,
  and pulls the image tagged with that version.
- After the very first build, set both GHCR packages (`fetcherr-amd64`, `fetcherr-aarch64`)
  to **Public** in GitHub → Packages → Package settings, so Home Assistant can pull them.
