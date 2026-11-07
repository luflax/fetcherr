# Fetcherr

Fetcherr exposes a REST API on port **8734** (you can change the host port in the add-on's *Network* section).

## Endpoints

| Method | Path         | Response                         |
|--------|--------------|----------------------------------|
| GET    | `/api/hello` | `{"message":"Hello, world!"}`    |
| GET    | `/health`    | `{"status":"ok"}` (watchdog)     |

Example:

```sh
curl http://homeassistant.local:8734/api/hello
```

## Options

| Option      | Default | Description                                          |
|-------------|---------|------------------------------------------------------|
| `log_level` | `info`  | One of `trace`, `debug`, `info`, `warn`, `error`.    |

## Talking to Home Assistant and other add-ons (planned)

- **Home Assistant Core API:** set `homeassistant_api: true` in `config.yaml`, then call
  `http://supervisor/core/api/...` with header `Authorization: Bearer $SUPERVISOR_TOKEN`.
- **Other add-ons:** add-ons share the internal `hassio` Docker network and are reachable by
  their hostname (shown on each add-on's info page, e.g. `a0d7b954-<slug>`) on their internal port.
