# ADR 0002: Offline via full-snapshot sync into on-device SQLite

Date: 2026-10-02
Status: accepted

## Context

Users should still see fountains/swim spots/temperatures when offline; the
dataset is small (hundreds of POIs, ~200 stations, latest measurements — a
few hundred KB). Live temps cannot be truly live offline, but stale values
with a visible timestamp are acceptable.

## Decision

- The app embeds SQLite (sqlx, same crate family as the server) and syncs a
  **full snapshot** from `GET /api/v1/snapshot`: on app start, on manual
  refresh, and every ~10 minutes while online.
- The snapshot carries `generated_at`; the app shows a staleness badge
  ("data from 14:20") whenever the cache is older than the last successful
  sync, and always when offline.
- No delta/patch machinery. If the dataset grows beyond comfort, revisit.

## Consequences

- Offline support is read-only: map + details work; SOS and helper features
  require connectivity (they are real-time by nature).
- The server's snapshot endpoint is trivially cacheable (single origin).
