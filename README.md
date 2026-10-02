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
| `just check` | type-check the backend           |
| `just test`  | run backend tests                |
| `just lint`  | clippy with warnings denied      |
| `just fmt`   | format the backend               |

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
