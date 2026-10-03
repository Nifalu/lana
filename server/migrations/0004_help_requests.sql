-- Ticket 05: SOS lifecycle, matching and SSE (ADR 0004).

-- An SOS: one person's request for help. location is where help is needed
-- (GPS fix or dropped pin); radius_m is the matching radius for this
-- request (default 500, per-request override). status follows the lifecycle
-- open → responded → resolved, or open → cancelled. requester_id and
-- responder_id are opaque device UUIDs and are NEVER served in API
-- responses (ADR 0004 anonymity: the requester only observes status
-- transitions); responder_id stays NULL until the first responder wins.
CREATE TABLE help_requests (
    id            UUID PRIMARY KEY,
    requester_id  UUID NOT NULL REFERENCES devices(id) ON DELETE CASCADE,
    status        TEXT NOT NULL CHECK (status IN ('open', 'responded', 'resolved', 'cancelled')),
    note          TEXT,
    location      GEOGRAPHY(POINT, 4326) NOT NULL,
    radius_m      DOUBLE PRECISION NOT NULL CHECK (radius_m > 0),
    responder_id  UUID REFERENCES devices(id),
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    CHECK (requester_id <> responder_id)
);

CREATE INDEX help_requests_status_idx ON help_requests (status);
CREATE INDEX help_requests_geom_idx ON help_requests USING GIST (location);

-- The "originally notified" set: the devices that matched when the request
-- was created. Later status-change fan-outs still reach them even if their
-- live location or windows have changed since (ADR 0004).
CREATE TABLE help_request_notified (
    help_request_id UUID NOT NULL REFERENCES help_requests(id) ON DELETE CASCADE,
    device_id       UUID NOT NULL REFERENCES devices(id) ON DELETE CASCADE,
    PRIMARY KEY (help_request_id, device_id)
);
