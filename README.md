# lana

## Prerequisites

- [Nix](https://nixos.org/download) with flakes enabled:

  ```sh
  mkdir -p ~/.config/nix
  echo "experimental-features = nix-command flakes" >> ~/.config/nix/nix.conf
  ```

## Develop

```sh
nix develop   # dev shell: rust, cargo-tauri, node, sqlite, just
just dev      # run the app (first build takes a while)
```

| Command      | What it does                     |
|--------------|----------------------------------|
| `just dev`   | run the app in dev mode          |
| `just serve` | run the backend API server       |
| `just check` | type-check the Rust workspace    |
| `just test`  | run backend tests                |
| `just lint`  | clippy with warnings denied      |
| `just fmt`   | format the Rust workspace        |

## Backend server

The backend lives in `server/` as a workspace sibling of the Tauri app
(`src-tauri/`). One binary, three modes (`serve` / `import` / `poll`); see
`server/README.md` for the full API reference.

- The server applies its own SQL migrations (`server/migrations/`) to the
  database on startup; PostGIS is bootstrapped by the first migration.
- `GET /api/v1/snapshot` returns the offline-sync snapshot: `pois` and
  `stations` as GeoJSON FeatureCollections (coordinates always `[lon, lat]`)
  plus a `generated_at` timestamp. Currently empty – data comes with the
  import ticket.
- Binds `0.0.0.0:8080` by default; override with `LANA_BIND_ADDR`.
- CORS is permissive (prototype).

## Notes

- Frontend framework is not chosen yet; `frontend/dist` holds a static
  placeholder that `just dev` serves.
- **The app has no Postgres.** It is a client of the server (ADR 0001) with
  an embedded SQLite offline cache (ADR 0002) in the platform app-data dir
  (`src-tauri/migrations/` holds the cache schema, applied at startup).
  The Tauri commands `get_device_id`, `get_server_url` / `set_server_url`,
  `sync_now`, `list_pois`, `list_stations` and `get_cache_info` cover
  device identity, server configuration, full-snapshot sync (wholesale
  replace) and cached queries; after one successful `sync_now` the app
  keeps working offline, with `get_cache_info` driving the staleness badge.
  Only the **server** still needs the dev Postgres:
  PostgreSQL (+ PostGIS). The dev shell exports `DATABASE_URL`
  (`postgres://lana:lana@127.0.0.1:5432/lana`). First time, run
  `just db-init`, `just db-start`, `just db-createdb`; stop with `just db-stop`.
  Inspect the DB with `psql "$DATABASE_URL"`. Schema changes are SQL files in
  `server/migrations/`, applied on server startup.
- The Tauri CLI is invoked as `cargo tauri …` (nixpkgs ships it as
  `cargo-tauri`).
- No Nix? Install Rust, Node, and the
  [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/) manually —
  the package commands on that page replace everything the dev shell provides.
