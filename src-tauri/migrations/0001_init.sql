-- Demo schema so the DB is exercised from day one.
-- Replace with the real schema; add new migrations as separate numbered files.
CREATE TABLE IF NOT EXISTS notes (
    id   BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    text TEXT NOT NULL
);

-- GIS work runs on PostGIS (the dev shell's postgresql ships it on Linux).
-- Uncomment once the schema actually stores geometries:
-- CREATE EXTENSION IF NOT EXISTS postgis;
