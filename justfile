set working-directory := 'src-tauri'

default:
    @just --list

# Run the Tauri app in dev mode
dev:
    cargo tauri dev

# Type-check the Rust backend
check:
    cargo check

# Run backend tests
test:
    cargo test

# Lint the Rust backend
lint:
    cargo clippy -- -D warnings

# Format the Rust backend
fmt:
    cargo fmt
