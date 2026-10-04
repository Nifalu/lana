-- lana app cache schema (embedded SQLite, ticket 06 - ADR 0002).
--
-- The app is a client: it owns no Postgres, only this local cache that is
-- replaced wholesale by a full-snapshot sync. Tables:
--
-- - settings  - on-device key/value store (device id, server base URL)
-- - pois      - cached POIs (kind/name/source plus point or polygon
--   geometry as GeoJSON text; lon/lat duplicate the representative point
--   for bbox filtering)
-- - stations  - cached measurement stations with their latest reading
-- - cache_meta - single row: last successful sync + snapshot generated_at

CREATE TABLE settings (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);

CREATE TABLE pois (
    id         INTEGER PRIMARY KEY,
    kind       TEXT NOT NULL,
    name       TEXT NOT NULL,
    source     TEXT NOT NULL,
    source_id  TEXT,
    lon        REAL NOT NULL,
    lat        REAL NOT NULL,
    geometry   TEXT NOT NULL,
    properties TEXT NOT NULL
);
CREATE INDEX pois_kind_idx ON pois (kind);

CREATE TABLE stations (
    id            TEXT PRIMARY KEY,
    kind          TEXT NOT NULL,
    name          TEXT NOT NULL,
    source        TEXT NOT NULL,
    lon           REAL NOT NULL,
    lat           REAL NOT NULL,
    geometry      TEXT NOT NULL,
    temperature_c REAL,
    measured_at   TEXT
);

CREATE TABLE cache_meta (
    id           INTEGER PRIMARY KEY CHECK (id = 1),
    last_sync_at TEXT,
    generated_at TEXT
);

INSERT INTO cache_meta (id, last_sync_at, generated_at) VALUES (1, NULL, NULL);
