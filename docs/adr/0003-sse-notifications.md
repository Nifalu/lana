# ADR 0003: SSE for helper notifications

Date: 2026-10-02
Status: accepted

## Context

When an SOS is created, matching helpers nearby must learn about it during
the demo. Options: foreground polling, Server-Sent Events, platform push
(FCM/APNs). Push needs Firebase projects per platform — not hackathon
scope. Polling works but wastes requests and feels laggy.

## Decision

One SSE endpoint, `GET /api/v1/events?device_id=<uuid>`, kept open by the
app while in foreground. Event types:

- `help_request_new` — sent to devices whose last known location or whose
  *active* recurring helper window matches the request (radius, see
  ADR-0004). Payload: the request (id, location, note, created_at).
- `help_request_updated` — status changes on a request: sent to the
  requester (their own request) and to notified helpers.

Cloudflare tunnels pass SSE; `axum` has first-class SSE support. If SSE
proves flaky on the event WLAN, the same queries exist as plain REST
endpoints (`GET /api/v1/help-requests?near=…`) as an instant fallback.

## Consequences

- No notification arrives when the app is backgrounded/killed (acceptable
  for the prototype; real push is a post-hackathon step).
- The server keeps in-memory per-connection state; restarts drop streams,
  clients must reconnect with the EventSource built-in retry.
