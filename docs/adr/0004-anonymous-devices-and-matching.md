# ADR 0004: Anonymous devices, helper windows, and matching

Date: 2026-10-02
Status: accepted

## Context

No accounts, no registration, privacy by default. Yet the help platform
must know *who is nearby* to notify potential helpers, and helpers may not
want to share a live location.

## Decision

- **Identity**: every app install generates a UUID v4 on first launch and
  stores it on-device. It is sent as `device_id` with every request that
  needs identity. Server table `devices(id, created_at, last_seen_at,
  last_location?, is_helper, …)`. No names, no profiles.
- **Anonymity of SOS**: a requester only ever learns that "someone is
  coming" (`status=open → responded`). Responder identity stays server-side.
- **Helper availability** (instead of live location): recurring weekly
  windows — `helper_windows(device_id, active, weekday, start_time,
  end_time, location, radius_m)` in Europe/Zurich local time. `active` is
  the user's vacation toggle. Helpers may *additionally* share a live
  location (`devices.last_location`) — then proximity uses it.
- **Matching** at SOS creation, radius R = 500 m default (const, tunable):
  a helper matches if (a) `last_location` within R of the SOS point and
  recently seen, or (b) an `active` window whose point is within R and
  whose weekday/time-of-day (Europe/Zurich) contains "now".

## Consequences

- Matching quality depends on honest windows; that is fine for the
  prototype and avoids continuous tracking.
- Luc's ongoing research on better proximity/notification strategies can
  replace the query behind matching without changing the API shape.
