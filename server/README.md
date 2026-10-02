# lana server

Rust backend for lana: REST API (`serve` mode) + idempotent open-data
import (`import` mode, ticket 02). Rust, axum, sqlx against PostgreSQL
with PostGIS.

```sh
just db-start && just db-createdb   # once
just serve                          # migrations run on startup
```

The server applies the SQL migrations in `migrations/` on startup (tracked
in `_sqlx_server_migrations` so it can share a dev database with the Tauri
app). Never edit an applied migration – add a new numbered file.

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

### Help requests (SOS)

The anonymous help platform (ADR 0004). A person feeling unwell creates a
request with their location (GPS fix or dropped pin) and an optional short
note; nearby helpers are matched and notified over SSE. There are no
requester/responder identity fields anywhere on the wire – parties only
observe status transitions.

- `POST /api/v1/help-requests` – create, `201`. Payload:
  `{ "device_id": "…", "location": {"lon": …, "lat": …}, "note": "…", "radius_m": 500 }`
  (`note` optional, ≤ 500 chars; `radius_m` optional, default 500). The
  requester device is auto-registered when unknown; status starts `open`.
  At creation the server matches helpers: devices sharing a live location
  within the radius and seen in the last 24 h, or with an **active** window
  whose weekday/time-of-day (Europe/Zurich) contains now and whose window
  point lies within the radius. Matched helpers receive `help_request_new`.
- `GET /api/v1/help-requests?status=open&near=7.59,47.56&radius_m=500` –
  list for the helper map; all filters optional.
- `POST /api/v1/help-requests/{request_id}/respond` – payload
  `{ "device_id": "…" }`. First responder wins → `responded`; the same
  responder repeating is idempotent (`200`); a different device gets `409`.
- `POST /api/v1/help-requests/{request_id}/resolve` – requester or
  responder → `resolved`.
- `POST /api/v1/help-requests/{request_id}/cancel` – requester only, only
  while `open` → `cancelled`.

Help-request document (also the SSE event data):

```json
{
  "id": "…",
  "status": "open",
  "note": "dizzy, need water",
  "location": { "lon": 7.5886, "lat": 47.5596 },
  "radius_m": 500.0,
  "created_at": "2026-10-02T12:00:00Z",
  "updated_at": "2026-10-02T12:00:00Z"
}
```

### Events (SSE)

`GET /api/v1/events?device_id=<uuid>` – server-sent events addressed to
this device (ADR 0003):

- `help_request_new` – a new SOS the device was matched for;
- `help_request_updated` – a status change of a request the device is a
  party to (requester/responder) or was originally notified about.

Event data is the help-request document above. The hub is in-memory and
per-process: notifications reach devices whose stream is open when the
event is published (SSE only for the prototype, no push).

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
