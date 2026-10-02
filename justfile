pgdata := '.pgdata'

default:
    @just --list

# Run the backend API server (needs the dev database: just db-start)
serve:
    cargo run -p lana-server -- serve

# Run the Tauri app in dev mode (needs the dev database: just db-start)
dev: fe-build
    cargo tauri dev

# Build the frontend if frontend/dist is missing (tauri codegen needs it)
fe-build:
    #!/bin/sh
    if [ ! -f frontend/dist/index.html ]; then
        cd frontend && npm ci && npm run build
    fi

# Type-check the Rust workspace (Tauri app + server)
check: fe-build
    cargo check --workspace

# Run backend tests (needs the dev database running: just db-start)
test: fe-build
    cargo test --workspace

# Lint the Rust workspace
lint: fe-build
    cargo clippy --workspace -- -D warnings

# Format the Rust workspace
fmt:
    cargo fmt --all

# Create a local Postgres cluster in .pgdata (first-time setup)
db-init:
    initdb -D {{pgdata}} --auth=trust -U lana

# Start the local Postgres cluster (socket in /tmp, TCP on 127.0.0.1:5432)
db-start:
    pg_ctl -D {{pgdata}} -l {{pgdata}}.log -o "-k /tmp" start

# Create the dev database (run once, after db-init + db-start)
db-createdb:
    createdb -h 127.0.0.1 -U lana lana

# Stop the local Postgres cluster
db-stop:
    pg_ctl -D {{pgdata}} stop
