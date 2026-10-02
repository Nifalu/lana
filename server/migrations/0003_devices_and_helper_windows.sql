-- Ticket 04: anonymous devices and helper windows (ADR 0004).

-- Devices are identified by a client-generated UUID v4 sent as device_id:
-- no accounts, no secrets. last_location is the optional shared live
-- location; NULL when the device chooses not to share one.
CREATE TABLE devices (
    id            UUID PRIMARY KEY,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    last_seen_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    is_helper     BOOLEAN NOT NULL DEFAULT FALSE,
    last_location GEOGRAPHY(POINT, 4326)
);

-- Recurring weekly availability windows for helpers. weekday is
-- 0=Monday..6=Sunday; start_time/end_time are local Europe/Zurich wall
-- times (time-of-day only; applying the timezone is matching's job,
-- ticket 05). active is the vacation toggle: an inactive window stays
-- stored but never matches. Window points carry their own radius in meters.
CREATE TABLE helper_windows (
    id         BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    device_id  UUID NOT NULL REFERENCES devices(id) ON DELETE CASCADE,
    active     BOOLEAN NOT NULL DEFAULT TRUE,
    weekday    SMALLINT NOT NULL CHECK (weekday BETWEEN 0 AND 6),
    start_time TIME NOT NULL,
    end_time   TIME NOT NULL,
    location   GEOGRAPHY(POINT, 4326) NOT NULL,
    radius_m   DOUBLE PRECISION NOT NULL CHECK (radius_m > 0),
    label      TEXT NOT NULL,
    CHECK (start_time < end_time)
);

CREATE INDEX helper_windows_device_idx ON helper_windows (device_id);
CREATE INDEX helper_windows_geom_idx ON helper_windows USING GIST (location);
