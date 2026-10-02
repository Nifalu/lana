-- lana server schema, v2: polygon-capable POI geometries + station source.
--
-- Swim areas (dataset 100270) are Polygons and must keep their shape for map
-- drawing, so the POI geometry column widens from POINT to GEOMETRY (points
-- remain valid).
ALTER TABLE pois ALTER COLUMN geom TYPE geography(GEOMETRY, 4326);

-- Stations carry their origin dataset so the snapshot can expose a `source`
-- property per station feature, mirroring the POI table.
ALTER TABLE stations ADD COLUMN source TEXT NOT NULL DEFAULT '';
