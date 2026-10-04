-- DATABASE IF NOT EXISTS hackamrhein;

--CREATE EXTENSION IF NOT EXISTS postgis;
--CREATE EXTENSION IF NOT EXISTS pg_cron;

--CREATE TABLE IF NOT EXISTS users (
--    id          varchar(255) PRIMARY KEY,
    --name        text,
--    location    geography(Point, 4326),
--    location_updated_at TIMESTAMPTZ
--);

--CREATE INDEX IF NOT EXISTS users_location_gix ON users USING GIST (location);

--CREATE TABLE IF NOT EXISTS cool_places (
--    id      bigint PRIMARY KEY,
--    name    varchar(255),
--    location geography(Point, 4326)
--    );

--CREATE INDEX IF NOT EXISTS cool_places_location_gix ON users USING GIST (location);

--CREATE TABLE IF NOT EXISTS fountains (
--    id      bigint PRIMARY KEY,
--    name    varchar(255),
--    location geography(Point, 4326)
 --   );

--CREATE INDEX IF NOT EXISTS fountain_location_gix ON users USING GIST (location);

--SELECT cron.schedule(
 --   'delete-stale-users',
 --   '*/15 * * * *',
  --  $$
 --       DELETE FROM users
 --       WHERE location_updated_at < now() - interval '14 minutes';
 --   $$
--);
