# lana

lana is a heat-relief map and anonymous help platform for Basel
([Hack am Rhein](https://hackamrhein.ch) 2026, challenge #3): public drinking
fountains, Rhine swim areas, cool places, and live temperatures (air, Rhine
water, pools) on a map — plus a way to ask people nearby for help when the
heat gets to you. No account, no identity, no live location feed; the app
keeps working offline.

## Architecture

Client/server split: one
central backend owns all shared state; the Tauri app is a client with an
embedded SQLite cache.

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

- The server imports Basel open data (data.bs.ch) and polls live values
  every ~10 minutes; phones only ever talk to the server.
- Helper notifications are Server-Sent Events; devices are anonymous
  client-generated UUIDs.
- The API reference — routes, payloads, SSE event names — lives in
  [server/README.md](server/README.md).

## Getting started

Prerequisites: [Nix](https://nixos.org/download) with flakes enabled
(`experimental-features = nix-command flakes`). No Nix? Install Rust, Node,
the [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/), and
PostgreSQL with PostGIS yourself.

```sh
nix develop                                        # dev shell: rust, cargo-tauri, node, postgres+postgis, just
just db-init && just db-start && just db-createdb  # once: local dev Postgres
just serve                                         # backend API on http://127.0.0.1:8080 (terminal 1)
just dev                                           # the Tauri app (terminal 2)
```

The app defaults to `http://127.0.0.1:8080` and caches a full snapshot in
SQLite — after one sync it keeps working offline and shows how stale its
data is. The dev shell exports `DATABASE_URL`
(`postgres://lana:lana@127.0.0.1:5432/lana`); inspect it with
`psql "$DATABASE_URL"`.

## Deployment

The backend is a two-service docker compose stack (PostGIS + server) — the
same files on a laptop and on the team's Proxmox host:

```sh
docker compose up -d --build
curl -s http://localhost:8080/api/v1/snapshot | head -c 300; echo
docker compose run --rm server import   # load the static datasets (one-shot)
```

Migrations run on server startup; data survives restarts in the `pgdata`
volume (`docker compose down -v` deletes it). For phones, point a
[Cloudflare tunnel](https://developers.cloudflare.com/cloudflare-one/connections/connect-networks/)
at port 8080 and set the tunnel URL in the app.

## Development

| Command                                   | What it does                                        |
| ----------------------------------------- | --------------------------------------------------- |
| `just serve`                              | backend API (needs the dev database)                |
| `just dev`                                | Tauri app in dev mode                               |
| `just check` / `just lint` / `just fmt`   | type-check / clippy `-D warnings` / format          |
| `just test`                               | workspace tests (backend tests need the database)   |
| `just db-init` / `db-start` / `db-stop`   | local dev Postgres lifecycle                        |

The server is one binary with three modes: `serve` (API + SSE + background
poller, applies migrations on startup), `import` (idempotent refresh of the
static datasets), and `poll` (one manual poll cycle for demos/tests).
Environment: `DATABASE_URL` (required), `LANA_BIND_ADDR` (default
`0.0.0.0:8080`), `LANA_POLL_INTERVAL_SECS` (default 600).

Schema changes are plain SQL files in `server/migrations/`, applied in
filename order at server startup — never edit an applied migration.

## Documentation

- [server/README.md](server/README.md) — API reference, import/poll
  internals, test conventions
