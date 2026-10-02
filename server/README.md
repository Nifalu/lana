# lana server

Rust backend for lana: REST API (`serve` mode) + idempotent open-data
import (`import` mode, ticket 02) + background live-measurement poller
(ticket 03). Rust, axum, sqlx against PostgreSQL with PostGIS.

```sh
just db-start && just db-createdb   # once
just serve                          # migrations run on startup
```

The server applies the SQL migrations in `migrations/` on startup (tracked
in `_sqlx_server_migrations` so it can share a dev database with the Tauri
app). Never edit an applied migration – add a new numbered file.

## Modes

- `lana-server serve` – REST API + background poller (below).
- `lana-server import` – idempotent refresh of the static datasets
  (fountains, swim areas, air stations; cool places come from the committed
  seed). Safe to re-run any time.
- `lana-server poll` – **manual poll trigger**: one poll cycle of the live
  measurements, on demand (demos/tests instead of waiting for the timer).

## Background poller (ticket 03)

While `serve` runs, a tokio task fetches the live datasets from data.bs.ch
and upserts the **latest value per station** (first cycle immediately,
then every `LANA_POLL_INTERVAL_SECS`, default 600 – roughly 10 minutes):

- **Air temperature** (dataset 100009): pages newest-first; a station's
  newest row wins. Paging stops once two consecutive pages discover no new
  station (page cap 20 × 100 rows bounds the walk). Stations join on
  `name_original` – the ids imported from dataset 100082.
- **Rhine water temperature** (dataset 100046): 15-minute aggregates from
  the Rheinüberwachungsstation Weil am Rhein (RUES, sensor strand "Strang
  S3"); only the newest row is read. The dataset has no station id and no
  coordinates, so the poller owns one fixed station (`rues-s3`). Its
  position exists only as Swiss LV03 `611740 / 272310` (EPSG:21781) in a
  field description; it was transformed **once** to WGS84 with PostGIS
  `ST_Transform(ST_SetSRID(ST_MakePoint(611740, 272310), 21781), 4326)` →
  `7.5947299 / 47.6013689` and committed as a constant – never converted
  at runtime.
- **Gartenbäder pool temperatures** (dataset 100384): one row per pool per
  scraper run, joined on the pool `name`; the poller creates one station
  per pool (id: slugified name, e.g. `hallenbad-eglisee`).

Rhine/pool stations are created by the poller itself (kinds `water`/`pool`);
air stations embedded in measurement rows are ensured too, so `serve` shows
live temperatures even without a prior `import`. Upserts conflict on
`(station_id, measured_at)` and are ignored – idempotent, and a station
that has not reported keeps its last known value. Every cycle logs what it
fetched; failures are logged and retried on the next tick.

Environment: `LANA_POLL_INTERVAL_SECS` (seconds, > 0, default 600),
`LANA_ODS_BASE_URL` (data.bs.ch override for local experiments).

## API

Base path `/api/v1`. GeoJSON coordinate order is `[lon, lat]` everywhere;
points are stored as `GEOGRAPHY(POINT, 4326)`. CORS is permissive for the
prototype.

### Identity (ADR 0004)

There are no accounts or secrets: a device generates a UUID v4 once,
stores it locally, and sends it as `device_id` in the request path. The
path's device_id **is** the caller – every devices/windows handler scopes
its SQL to that id, so a device can only ever read or change its own rows.
Unknown and foreign resources are both reported as `404` (no probing).

### Devices

`PUT /api/v1/devices/{device_id}` – upsert (first call creates the row).
The payload is the device's current state; `location` null or omitted
means "not sharing a live location" (a previously shared location is
cleared on update).

```json
{ "is_helper": true, "location": { "lon": 7.5886, "lat": 47.5596 } }
```

Response `200`:

```json
{
  "device_id": "…",
  "is_helper": true,
  "location": { "lon": 7.5886, "lat": 47.5596 },
  "created_at": "2026-10-02T12:00:00Z",
  "last_seen_at": "2026-10-02T12:34:56Z"
}
```

### Helper windows

Recurring weekly availability, scoped to the calling device. Times are
Europe/Zurich local wall times (`"HH:MM"` or `"HH:MM:SS"`; always
`"HH:MM:SS"` in responses); `weekday` is `0=Monday..6=Sunday`; `radius_m`
is in meters and must be > 0; `end_time` must be strictly after
`start_time`; `location` must be within WGS84 bounds. Matching evaluates
the windows server-side (ticket 05).

- `POST /api/v1/devices/{device_id}/windows` – create, `201` (the
  referenced device must exist; `active` defaults to `true`)
- `GET /api/v1/devices/{device_id}/windows` – list the device's own windows
- `PATCH /api/v1/devices/{device_id}/windows/{window_id}` – partial update
  (absent fields keep their value; the merged state is revalidated;
  `active` is the vacation toggle and switches without deleting)
- `DELETE /api/v1/devices/{device_id}/windows/{window_id}` – `204`

Window document:

```json
{
  "id": 1,
  "device_id": "…",
  "active": true,
  "weekday": 0,
  "start_time": "09:00:00",
  "end_time": "17:00:00",
  "location": { "lon": 7.5886, "lat": 47.5596 },
  "radius_m": 500.0,
  "label": "Büro"
}
```

### Snapshot

`GET /api/v1/snapshot` – full offline-sync snapshot (GeoJSON
FeatureCollections for `pois` and `stations` + `generated_at`). Ticket 02
fills it from Postgres.

Station features carry `id`, `kind`, `name`, `source` plus their latest
measurement from the poller: `temperature_c` (number, °C) and `measured_at`
(RFC 3339) – both `null` while the station has never reported, last known
value once it has.

### Errors

Validation failures: `422` with `{"error": "…"}`. Unknown or foreign
resources: `404`. Malformed JSON/paths: `400` (axum defaults).

## Tests

HTTP-seam tests drive the axum router in-process (`tower::ServiceExt::oneshot`
– no sockets, no fixed ports) against a real, migrated Postgres. They skip
silently when `DATABASE_URL` is unset:

```sh
nix develop -c bash -c \
  'export DATABASE_URL=postgres://lana:lana@127.0.0.1:5432/lana; just test'
```
