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
- SQLite via bundled rusqlite. Schema changes are appended to `MIGRATIONS` in
  `src-tauri/src/db.rs`. Inspect the DB with `sqlite3` from the dev shell.
- The Tauri CLI is invoked as `cargo tauri …` (nixpkgs ships it as
  `cargo-tauri`).
- No Nix? Install Rust, Node, and the
  [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/) manually —
  the package commands on that page replace everything the dev shell provides.
