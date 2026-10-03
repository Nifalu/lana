# ADR 0001: Client/server split with a central axum backend

Date: 2026-10-02
Status: accepted

## Context

lana needs (a) a map of cooling POIs and live temperatures, ideally usable
offline, and (b) a help platform where a person feeling unwell sends an SOS
and *other devices* nearby get notified. The first Tauri scaffold connected
the app directly to a PostgreSQL server via `DATABASE_URL` — workable on a
dev desktop, impossible on phones (no Postgres server on an Android/iOS
device), and fundamentally unable to coordinate two devices.

## Decision

- One central backend (`server/`): Rust, axum, sqlx, PostgreSQL + PostGIS.
  It owns all shared state: POIs, measurements, devices, helper windows,
  help requests, and serves the REST API + one SSE endpoint.
- The Tauri app is a thin client. Its only local persistence is an embedded
  SQLite cache (ADR-0002). No Postgres on the device.
- Devices are anonymous: client-generated UUID v4, no login, no profiles.
  Requesters see only "someone is coming", never who (ADR-0004).

## Consequences

- Cross-device features (SOS coordination, helper discovery) live entirely
  server-side; the offline story is limited to read-only POI/temperature
  data, which is exactly where offline matters.
- The backend must be reachable from demo phones (Cloudflare tunnel to the
  team's Proxmox host; see README deployment section).
- `data.bs.ch` is only ever called by the server (importer + poller), never
  by apps. One origin to cache, one place to normalize.

## Alternatives considered

- On-device DB plus a sync layer for shared entities: rejected — solves a
  problem only the read-only data has, at high complexity for a hackathon.
- Real push notifications (FCM/APNs): rejected for the hackathon (needs
  Firebase setup per platform); SSE covers the demo (ADR-0003).
