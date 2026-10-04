lana

local aid notification assistant

lana is a heat-relief map and anonymous help platform for Basel (Hack am Rhein 2026, challenge #3): public drinking fountains, Rhine swim areas, cool places, and live temperatures (air, Rhine water, pools) on a map — plus a way to ask people nearby for help when the heat gets to you. No account, no identity, no live location feed; the app keeps working offline.

## What it does

- Heat-relief map (MapLibre) of fountains, Rhine swim spots, cool places + live air / Rhine / pool temperatures from data.bs.ch.
- Anonymous SOS: share location + note, nearby opted-in helpers are notified via SSE; first responder wins.

## Cross-platform via Tauri

- One codebase: Svelte frontend + Tauri shell runs as desktop app and mobile app (iOS/Android) from `frontend/dist`.
- Rust backend stays central (axum + PostgreSQL/PostGIS); only the server polls external open-data APIs.

## Privacy

No accounts, no identity: devices are random local UUIDs. Location is only shared while "Ich kann helfen" is on (5-min heartbeat), cleared on opt-out. Any location not refreshed for 15 minutes expires automatically — stale entries are deleted from the live-location store and ignored for SOS matching.

## Run it

```sh
nix develop
just db-init && just db-start && just db-createdb  # once
just serve  # API on :8090
just dev    # Tauri app (desktop/mobile)
```

Deploy: `docker compose up -d --build`, then `docker compose run --rm server import`.

## Layout

- `frontend/` – Svelte map / HUD / SOS UI
- `src-tauri/` – Tauri cross-platform shell
- `server/` – API, SSE, open-data import + poller ([docs](server/README.md))
- `backend/` – helper/location service (FastAPI live-location / closest-helpers API; details in [backend/README.md](backend/README.md))
