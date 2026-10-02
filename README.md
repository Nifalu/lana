# lana

Hack am Rhein 2026, challenge #3. Stack: **Tauri v2** (Rust backend) +
**SQLite**, Nix-flake dev environment. Frontend framework TBD.

> TODO: add the challenge description here once the team has access to
> https://hackamrhein.dev/challenges/3.

## Structure

```
├── flake.nix           # Nix dev environment (rust, node, sqlite, tauri-cli, GTK/WebKit deps)
├── justfile            # common tasks (dev, check, test, lint, fmt)
├── src-tauri/          # Rust backend (Tauri v2)
│   ├── src/main.rs     # entry point
│   ├── src/lib.rs      # app builder, commands, state
│   ├── src/db.rs       # SQLite open + append-only migrations (PRAGMA user_version)
│   ├── tauri.conf.json
│   └── capabilities/   # Tauri v2 permission grants
└── frontend/           # placeholder until the framework is chosen
    └── dist/           # static placeholder page so `tauri dev` runs today
```

## Getting started

Requires [Nix](https://nixos.org/download) with flakes enabled
(`experimental-features = nix-command flakes`).

```sh
nix develop     # drops you into the dev shell
just dev        # runs the Tauri app (first build takes a while)
```

`just check`, `just test`, `just lint`, `just fmt` work the same way.

On macOS the shell needs no GUI libraries (system WebKit is used); on Linux it
ships WebKitGTK and friends, so nothing has to be installed system-wide.
The Tauri CLI is invoked as `cargo tauri …` (nixpkgs ships it as `cargo-tauri`).

## Database

SQLite via `rusqlite` (bundled — no system libsqlite needed). The DB lives in
the OS app-data dir (`~/.local/share/dev.hackamrhein.lana/lana.db` on Linux).
Schema changes go through the append-only `MIGRATIONS` list in
`src-tauri/src/db.rs`.

Inspect it from the dev shell with `sqlite3` (see `just` recipes if needed).

## Frontend (TBD)

`frontend/dist` currently holds a static placeholder. When the framework is
chosen, replace it and update `build.frontendDist` plus
`beforeDevCommand`/`beforeBuildCommand` in `src-tauri/tauri.conf.json`.
