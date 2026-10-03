# lana

Basel heat map + anonymous help platform (Hack am Rhein 2026, challenge #3):
public fountains, Rhine swim areas, cool places, and live temperatures
(air / Rhine water / pools) on a map — plus a way to ask people nearby for
help when the heat gets to you, without an account, identity, or a live
location feed.

## Architecture

Client/server split (ADR 0001): one central backend owns all shared state;
the Tauri app is a client with an embedded SQLite offline cache (ADR 0002).

```
┌────────────────────────┐             ┌───────────────────────────────┐
│ Tauri app              │  REST       │ lana-server (Rust, axum)      │
│ (desktop now,          │────────────►│  • REST API under /api/v1     │
│  Android stretch)      │◄────────────│  • SSE events per device      │
│                        │    SSE      │  • background poller for      │
│ embedded SQLite cache  │             │    live data.bs.ch data       │
└────────────────────────┘             └──────────────┬────────────────┘
                                                      │
                                        ┌─────────────▼─────────────┐
                                        │  PostgreSQL + PostGIS     │
                                        │  (POIs, measurements,     │
                                        │   devices, help requests) │
                                        └───────────────────────────┘
```

- The app **never talks to Postgres** (it must run on phones); it keeps a
  full snapshot in SQLite and works offline with a staleness indicator.
- The server imports Basel open data (data.bs.ch) and polls live values
  every ~10 minutes; phones only ever talk to the server.
- Helper notifications are Server-Sent Events (ADR 0003); devices are
  anonymous client-generated UUIDs (ADR 0004).

## Repository layout

| Path              | What it is                                                   |
| ----------------- | ------------------------------------------------------------ |
| `server/`         | Backend crate (axum + sqlx + PostGIS). **API reference: `server/README.md`** |
| `src-tauri/`      | Tauri app shell (SQLite cache, commands, migrations)          |
| `frontend/`       | Svelte frontend (subtree-merged from the team's branch)       |
| `docs/adr/`       | Architecture decision records (0001–0004)                     |
| `docs/HANDOVER.md`| Who owns what, open tickets                                   |

## Develop (desktop is the guaranteed demo path)

Prerequisites: [Nix](https://nixos.org/download) with flakes enabled:

```sh
mkdir -p ~/.config/nix
echo "experimental-features = nix-command flakes" >> ~/.config/nix/nix.conf
```

First-time setup, then every day:

```sh
nix develop                          # dev shell: rust, cargo-tauri, node, postgres+postgis, just
just db-init && just db-start && just db-createdb   # once: local dev Postgres
just serve                           # terminal 1: API on http://127.0.0.1:8080 (migrations run on startup)
just dev                             # terminal 2: the Tauri app (talks to 127.0.0.1:8080 by default)
```

The dev shell exports `DATABASE_URL=postgres://lana:lana@127.0.0.1:5432/lana`.
Inspect the database with `psql "$DATABASE_URL"`; stop it with `just db-stop`.
Schema changes are plain SQL files in `server/migrations/`, applied in
filename order at server startup — never edit an applied migration.

No Nix? Install Rust, Node, and the
[Tauri prerequisites](https://v2.tauri.app/start/prerequisites/), plus
PostgreSQL with PostGIS, export `DATABASE_URL` yourself, and run the same
`just` recipes (or the underlying `cargo` commands).

### just recipes

| Command           | What it does                                            |
| ----------------- | ------------------------------------------------------- |
| `just dev`        | run the Tauri app in dev mode                           |
| `just serve`      | run the backend API server (needs the dev database)     |
| `just check`      | type-check the Rust workspace                           |
| `just test`       | run the workspace tests (backend tests need the database) |
| `just lint`       | clippy with warnings denied                             |
| `just fmt`        | format the Rust workspace                               |
| `just db-init` / `db-start` / `db-createdb` / `db-stop` | local dev Postgres lifecycle |

### Backend modes

One binary (`lana-server`), three modes — full details in `server/README.md`:

| Mode     | Purpose                                                                 |
| -------- | ----------------------------------------------------------------------- |
| `serve`  | REST API + SSE + background poller; **applies migrations on startup**   |
| `import` | idempotent refresh of the static datasets (fountains, swim areas, cool places, air stations) |
| `poll`   | one manual poll cycle of the live measurements (demos/tests)            |

Environment: `DATABASE_URL` (required), `LANA_BIND_ADDR` (default
`0.0.0.0:8080`), `LANA_POLL_INTERVAL_SECS` (default 600),
`LANA_ODS_BASE_URL` (data.bs.ch override).

Refresh the demo data in a running deployment:

```sh
docker compose run --rm server import   # static datasets (one-shot container)
docker compose run --rm server poll     # live measurements, one cycle
```

(The image's entrypoint is `lana-server`, so the mode is the only argument.
`docker compose exec server lana-server import` does the same inside the
already-running server container.)

## Deployment

The whole backend is a two-service docker-compose stack — same files on a
laptop and on the Proxmox host:

```sh
docker compose up -d --build         # builds the server image, starts db + server
curl -s http://localhost:8080/api/v1/snapshot | head -c 300; echo
```

- The **server container applies the migrations on startup** (its `serve`
  mode always does) and serves the API on **8080**.
- `import` is a separate one-shot command (see above); the poller keeps live
  values fresh automatically while `serve` runs.
- Data survives restarts in the named **`pgdata` volume**; only
  `docker compose down -v` deletes it.

**Proxmox + Cloudflare tunnel:** run the stack on a Proxmox VM/LXC, then
point a `cloudflared` tunnel at `http://localhost:8080` and route a public
hostname (e.g. `lana.example.com`) to it. Phones then reach the server at
the tunnel URL — set it once per device in the app (`set_server_url`), see
the Android checklist below. HTTPS at the tunnel edge is what makes GPS +
SSE work on real devices without wrangling certificates.

## Android APK checklist (stretch goal)

Desktop is the guaranteed demo path. Two Android phones is the stretch goal —
the following checklist is written so anyone can produce the APK.

### One-time setup

1. **Android SDK + NDK** — install [Android Studio](https://developer.android.com/studio)
   (or the command-line tools) and, via *SDK Manager → SDK Tools*, tick:
   - Android SDK Platform (API 34), SDK Platform-Tools, SDK Build-Tools
   - **NDK (Side by side)** — current LTS version
   - CMake (if offered alongside the NDK)
2. **Rust Android targets** (on a typical 64-bit ARM phone):

   ```sh
   rustup target add aarch64-linux-android
   ```

3. **Environment variables** — put in your shell profile, with versions
   matching your SDK Manager choices:

   ```sh
   export ANDROID_HOME="$HOME/Android/Sdk"                 # macOS: ~/Library/Android/sdk
   export NDK_HOME="$ANDROID_HOME/ndk/<version>"           # e.g. 28.2.13676358
   export PATH="$PATH:$ANDROID_HOME/platform-tools"        # gives you `adb`
   ```

4. Java 17+ (bundled with Android Studio) — set `JAVA_HOME` if `cargo tauri`
   complains.

### Build the APK

```sh
nix develop                          # or your local Rust + cargo-tauri install
cargo tauri android init             # once: generates src-tauri/gen/android
cargo tauri android build --debug    # produces an installable debug APK
```

The APK lands under `src-tauri/gen/android/app/build/outputs/apk/universal/debug/`
(`--debug`; swap for `build` for a release APK, which needs signing setup).

### Install & demo on two phones

1. Enable *Developer options → USB debugging* on both phones, plug in via
   USB, accept the fingerprint dialog.
2. Install: `adb install <path-to>.apk` (or `adb install -s` to target a
   specific device when both are plugged in; `adb devices` lists them).
3. **Point the app at the server**: the app defaults to `http://127.0.0.1:8080`,
   which is wrong on a phone. In the app, set the server URL (`set_server_url`
   → the setting field in the UI) to the **tunnel URL** from the deployment
   section, e.g. `https://lana.example.com`.
4. Demo flow: phone A sends an SOS from the map; phone B (registered as a
   helper, nearby or on schedule) receives it via SSE and responds; phone A
   sees "someone is coming"; A resolves (or cancels).

If Android blocks at any step, fall back to the desktop path — `just dev`
against a deployed server gives the identical flow on two laptop windows.

## Checks

```sh
nix develop -c bash -c 'export DATABASE_URL=postgres://lana:lana@127.0.0.1:5432/lana; just check'
# same for: just lint, just test   (backend tests skip silently without DATABASE_URL)
```

## Docs map

- `server/README.md` — backend API reference (routes, payloads, SSE event
  names), import/poller internals, test conventions.
- `docs/adr/0001` — client/server split · `0002` — full-snapshot offline
  cache · `0003` — SSE notifications · `0004` — anonymous devices & matching.
- `docs/HANDOVER.md` — current state, owners, open tickets.
