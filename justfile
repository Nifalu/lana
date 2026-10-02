default:
    @just --list

# Run the Tauri app in dev mode
dev:
    cargo tauri dev

# Type-check the Rust backend
check:
    cargo check --manifest-path src-tauri/Cargo.toml

# Run backend tests (needs the dev database running: just db-start)
test:
    cargo test --manifest-path src-tauri/Cargo.toml

# Lint the Rust backend
lint:
    cargo clippy --manifest-path src-tauri/Cargo.toml -- -D warnings

# Format the Rust backend
fmt:
    cargo fmt --manifest-path src-tauri/Cargo.toml

# Create a local Postgres cluster in .pgdata (first-time setup)
db-init:
    initdb -D .pgdata --auth=trust -U lana

# Start the local Postgres cluster (socket in /tmp, TCP on 127.0.0.1:5432)
db-start:
    pg_ctl -D .pgdata -l .pgdata.log -o "-k /tmp" start

# Create the dev database (run once, after db-init + db-start)
db-createdb:
    createdb -h 127.0.0.1 -U lana lana

# Stop the local Postgres cluster
db-stop:
    pg_ctl -D .pgdata stop
