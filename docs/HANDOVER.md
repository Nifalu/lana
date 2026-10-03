# Handover — lana backend foundation

Hack am Rhein 2026, challenge #3 (Basel heat map + anonymous help platform).
This note is for whoever picks the project up tomorrow without the original
author present.

## State

All backend agent tickets (01–07) are merged into `feat/backend-foundation`:

- **Server** (`server/`): axum + sqlx on PostgreSQL/PostGIS. One binary,
  three modes (`serve` = API + SSE + poller with migrations on startup,
  `import` = idempotent dataset refresh, `poll` = one manual poll cycle).
- **Data**: Basel open data imported from data.bs.ch (fountains, Rhine swim
  areas, air stations, pool temperatures); Rhine water + pools kept fresh by
  the background poller; cool places committed as a geocoded seed file.
- **API**: REST under `/api/v1` + SSE events — fully documented in
  [`server/README.md`](../server/README.md).
- **App** (`src-tauri/`): Tauri client with an embedded SQLite cache; the
  Tauri commands (`get_device_id`, `get/set_server_url`, `sync_now`,
  `list_pois`, `list_stations`, `get_cache_info`) are the integration
  surface for the frontend.
- **Deployment**: `docker-compose.yml` (PostGIS + server image) runs on a
  laptop or the Proxmox host behind a Cloudflare tunnel — see the
  [README deployment section](../README.md#deployment).

Read the ADRs first — they record *why* the system looks the way it does:
`docs/adr/0001`–`0004` (client/server split, snapshot offline cache, SSE
notifications, anonymous devices & matching).

## Who owns what

| Person | Scope |
| ------ | ----- |
| **Nico** | Frontend wiring (ticket 08): typed API client + stores, map layers, staleness badge, then SOS/helper UI. Plus the Android APK, following the [Android checklist](../docs/ANDROID.md). |
| **Luc** | Proximity/notification research (how matching radius + windows should really behave; what replaces SSE later) and routing (Valhalla demo lives on a separate branch — track it as its own ticket). |
| *(open)* | Ticket 09, dataset normalization review — a joint session; decide fountain classification, dead-station filtering, snapshot trimming. |

## Open tickets

Both are `ready-for-human` in `.scratch/backend-foundation/issues/`:

- **08 — Frontend wiring: API client and SOS/map hooks** (Nico). Start with
  the read-only parts (snapshot fetch, layers, staleness); the SOS and
  helper-mode UI lands on top — the backend side (05) is done.
- **09 — Dataset normalization review** (joint, with Ruben when available).
  Fountains currently all have `kind = fountain` (the dataset has no type
  field); dead stations are not filtered; the snapshot may carry media URLs
  we don't need.

## Demo recipe (fastest path)

```sh
docker compose up -d --build                    # backend on :8080
docker compose run --rm server import            # fill POIs once
just dev                                        # desktop app (guaranteed path)
```

Two desktop windows (one as helper, one sending an SOS) demo the full
SSE round-trip; two Android phones via the APK checklist is the stretch goal.

## Known limitations (accepted for the prototype)

- The SSE hub is in-memory and per-process — restart drops open streams,
  no offline delivery, no real push (FCM/APNs).
- No authentication, rate limiting, or privacy audit; all API data is public
  by design.
- Full-snapshot sync only (no deltas); fine at Basel dataset sizes.
- Matching radius is a constant (500 m default, per-request override);
  helper windows evaluate Europe/Zurich local time server-side.
