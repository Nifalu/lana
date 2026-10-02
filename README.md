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
(`src-tauri/`) and talks to the same dev Postgres. One binary, two modes:

```sh
just serve                          # apply migrations, then serve the REST API
cargo run -p lana-server -- import  # open-data import (not implemented yet)
```

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
- PostgreSQL (+ PostGIS for GIS work; the dev shell ships PostGIS on Linux).
  The dev shell exports `DATABASE_URL`
  (`postgres://lana:lana@127.0.0.1:5432/lana`). First time, run
  `just db-init`, `just db-start`, `just db-createdb`; stop with `just db-stop`.
  Inspect the DB with `psql "$DATABASE_URL"`.
  Schema changes are SQL files in `src-tauri/migrations/` – sqlx embeds them at
  compile time and applies them at app startup (the app refuses to start if
  the database is unreachable).
- The Tauri CLI is invoked as `cargo tauri …` (nixpkgs ships it as
  `cargo-tauri`).
- No Nix? Install Rust, Node, and the
  [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/) manually —
  the package commands on that page replace everything the dev shell provides.
