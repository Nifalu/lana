# lana

## local aid notification assistant

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

## Troubleshooting (Linux)

**`Could not create default EGL display: EGL_BAD_PARAMETER. Aborting...`** (blank/frozen
window; the render process crashes in a loop): nix-built Mesa only searches
`/run/opengl-driver` — a NixOS-only symlink — for its DRI/GBM drivers. On non-NixOS
hosts (Fedora here) that path doesn't exist, so WebKit's render process can't create
its EGL display. The flake env fixes this by pointing `GBM_BACKENDS_PATH`,
`LIBGL_DRIVERS_PATH` and `__EGL_VENDOR_LIBRARY_FILENAMES` into the nix store and
disabling the WebKit sandbox (it would hide those paths from the render process).
Just make sure the direnv/nix shell is loaded (`direnv reload` after flake changes).

Alternatively, keep the sandbox by making the NixOS-style symlink yourself
(adapt the mesa store path to `nix store --realise` output of your flake):
`sudo ln -sfn /nix/store/<mesa> /run/opengl-driver` + drop
`WEBKIT_DISABLE_SANDBOX_THIS_IS_DANGEROUS` from the flake.

**GStreamer `appsink`/`autoaudiosink` warnings at startup**: cosmetic — the nix env
has no GStreamer plugins, so WebKit media playback is unavailable. The app doesn't
use it.

## Layout

- `frontend/` – Svelte map / HUD / SOS UI
- `src-tauri/` – Tauri cross-platform shell
- `server/` – API, SSE, open-data import + poller ([docs](server/README.md))
- `backend/` – helper/location service (FastAPI live-location / closest-helpers API; details in [backend/README.md](backend/README.md))
