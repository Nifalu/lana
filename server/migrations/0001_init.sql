-- lana server schema, v1: skeleton tables for POIs and stations.
-- PostGIS MUST be bootstrapped before any GEOGRAPHY column exists.
CREATE EXTENSION IF NOT EXISTS postgis;

-- All points of interest: fountains, Rhine swim areas, curated cool places.
-- The kind column starts as fountain-only for imported fountains (dataset
-- 100008); classification is a follow-up review.
CREATE TABLE pois (
    id         BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    kind       TEXT NOT NULL CHECK (kind IN ('fountain', 'swim_area', 'cool_place')),
    name       TEXT NOT NULL,
    geom       GEOGRAPHY(POINT, 4326) NOT NULL,
    properties JSONB NOT NULL DEFAULT '{}'::jsonb,
    source     TEXT NOT NULL,
    source_id  TEXT,
    UNIQUE (source, source_id)
);
CREATE INDEX pois_kind_idx ON pois (kind);
CREATE INDEX pois_geom_idx ON pois USING GIST (geom);

-- Measurement stations (Smart Climate air stations, Rhine water values,
-- garden pool temperatures).
CREATE TABLE stations (
    id   TEXT PRIMARY KEY,
    kind TEXT NOT NULL CHECK (kind IN ('air', 'water', 'pool')),
    name TEXT NOT NULL,
    geom GEOGRAPHY(POINT, 4326) NOT NULL
);
CREATE INDEX stations_geom_idx ON stations USING GIST (geom);

-- Time series of station measurements; the snapshot serves the latest value
-- per station.
CREATE TABLE measurements (
    id            BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    station_id    TEXT NOT NULL REFERENCES stations(id) ON DELETE CASCADE,
    measured_at   TIMESTAMPTZ NOT NULL,
    temperature_c DOUBLE PRECISION,
    UNIQUE (station_id, measured_at)
);
