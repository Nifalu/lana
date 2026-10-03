# lana server

Rust backend for lana: REST API (`serve` mode) + idempotent open-data
import (`import` mode, ticket 02) + background live-measurement poller
(ticket 03). Rust, axum, sqlx against PostgreSQL with PostGIS.

```sh
just db-init && just db-start && just db-createdb   # once (local dev Postgres)
just serve                          # migrations run on startup
```

The same stack runs as a docker-compose deployment (postgis image + this
server, API on 8090, data in a named volume) – see the root README,
"Deployment".

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

Other `serve` settings: `LANA_BIND_ADDR` (default `0.0.0.0:8090`) and
`LANA_HELPER_API_URL` (base URL of the helper API; unset/empty =
disabled, see "Helper matching and the helper API").

## API

Base path `/api/v1`. GeoJSON coordinate order is `[lon, lat]` everywhere;
points are stored as `GEOGRAPHY(POINT, 4326)`. CORS is permissive for the
prototype.

### Identity (ADR 0004)

There are no accounts or secrets: a device generates a UUID v4 once,
stores it locally, and sends it as `device_id` in the request path. The
path's device_id **is** the caller – every devices handler scopes
its SQL to that id, so a device can only ever read or change its own rows.
Unknown and foreign resources are both reported as `404` (no probing).

### Devices

`PUT /api/v1/devices/{device_id}` – upsert (first call creates the row).
The payload is the device's current state: `is_helper` is the app's
"Ich kann helfen" opt-in (the app sends it with its location every
5 minutes while on, and `{ "is_helper": false, "location": null }` when
off); `location` null or omitted means "not sharing a live location" (a
previously shared location is cleared on update). When the helper API is
configured (below), a shared location is also forwarded to it.

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

### Snapshot

`GET /api/v1/snapshot` – full offline-sync snapshot (GeoJSON
FeatureCollections for `pois` and `stations` + `generated_at`). Ticket 02
fills it from Postgres. Station features carry `id`, `kind`, `name`,
`source` plus their latest measurement from the poller: `temperature_c`
(number, °C) and `measured_at` (RFC 3339) – both `null` while the station
has never reported, last known value once it has.

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
  At creation the server matches helpers (see "Helper matching" below);
  matched helpers receive `help_request_new`.
- `GET /api/v1/help-requests?status=open&near=7.59,47.56&radius_m=500&exclude_device_id=…` –
  list for the helper map; all filters optional. `exclude_device_id` leaves
  out requests that device created itself. Response is a JSON **array**
  of help-request documents (possibly empty).
- `POST /api/v1/help-requests/{request_id}/respond` – payload
  `{ "device_id": "…" }`. First responder wins → `responded`; the same
  responder repeating is idempotent (`200`); a different device gets `409`.
  A responder device unknown to the server is auto-registered.
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

### Helper matching and the helper API

Everyone can help – there are no availability windows. A device is a
helper candidate when it opted in (`is_helper`) and shares a location. The
requester is never matched against their own SOS.

A separate service (the "helper API", FastAPI, owned by a teammate) owns
live device locations and the closest-helpers query. It is optional and
enabled by `LANA_HELPER_API_URL` (e.g. `https://lana.heitzli.ch`; unset or
empty = disabled, trailing slash tolerated):

- **Forwarding.** After `PUT /api/v1/devices/{device_id}` has written the
  database, a shared location is sent to `POST {url}/location?device_id=<uuid>`
  with body `{"longitude": …, "latitude": …}` from a spawned task. The PUT
  never waits for it and never fails because of it (failures are logged).
  A null location forwards nothing – the API has no delete, so opted-out
  devices are filtered at match time.
- **Matching with the helper API.** Before the SOS transaction opens,
  `POST {url}/get_closest_helpers` with the SOS location returns the
  devices with a location updated in the last 15 minutes within 500 m,
  nearest first (`[{ "id": "<device id>", "distance_m": …, … }]`; only `id`
  is read, ids that are not UUIDs are skipped). Inside the transaction only
  candidates that are `is_helper`, still have a stored location and are not
  the requester are recorded as notified and sent `help_request_new`. The
  API's radius (500 m) decides; the request's `radius_m` is still stored and
  returned but not used for the match.
- **Local matching** (no helper API configured, or the call failed – error,
  timeout, non-2xx or an unparseable body, logged as a warning): `is_helper`
  devices other than the requester with a stored location seen within the
  last 15 minutes (`last_seen_at`) and within `radius_m` of the SOS.

HTTP timeouts to the helper API are 2 s to connect and 3 s per request.
The SOS lifecycle (create, respond, resolve, cancel, SSE fan-out) is
always handled by this server.

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
