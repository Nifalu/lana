# Multi-stage build of the lana server (the only workspace member that ships
# as a container). The Tauri app is built separately (see README, "Android
# APK checklist"); it never needs Postgres and does not run in this stack.

# ---- Build stage -----------------------------------------------------------
FROM rust:1-slim AS build
WORKDIR /build

# Copy the manifests first so the dependency layers stay cached while server
# code changes. The workspace root manifest lists both members, so cargo needs
# both member manifests to load the workspace – but only the server is built.
COPY Cargo.toml Cargo.lock ./
COPY src-tauri/Cargo.toml src-tauri/Cargo.toml
COPY server/Cargo.toml server/Cargo.toml

# Placeholder sources: cargo refuses to load a workspace member whose sources
# are missing (auto-discovery of src/lib.rs / src/main.rs), so create dummies,
# then compile the release dependencies once into the layer cache.
RUN mkdir -p src-tauri/src server/src \
    && echo "" > src-tauri/build.rs \
    && echo "" > src-tauri/src/lib.rs \
    && echo "fn main() {}" > src-tauri/src/main.rs \
    && echo "" > server/src/lib.rs \
    && echo "fn main() {}" > server/src/main.rs \
    && cargo build --release -p lana-server

# Real server code (migrations are embedded by sqlx::migrate! at compile
# time). COPY preserves mtimes, which can predate the cached dummy artifacts,
# so touch the sources to force the rebuild.
COPY server/ server/
RUN find server/src server/migrations -type f -exec touch {} + \
    && cargo build --release -p lana-server

# ---- Runtime stage ---------------------------------------------------------
FROM debian:bookworm-slim

# ca-certificates: the poller/import fetch datasets from https://data.bs.ch.
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --system --uid 10001 lana

COPY --from=build /build/target/release/lana-server /usr/local/bin/lana-server

USER lana
EXPOSE 8090

# `serve` applies the SQL migrations in server/migrations/ on startup, then
# serves the API + SSE and runs the background measurement poller.
ENTRYPOINT ["lana-server"]
CMD ["serve"]
