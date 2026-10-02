-- DATABASE IF NOT EXISTS hackamrhein;

CREATE EXTENSION IF NOT EXISTS postgis;
CREATE EXTENSION IF NOT EXISTS pg_cron;

CREATE TABLE IF NOT EXISTS users (
    id          bigint PRIMARY KEY,
    name        text,
    location    geography(Point, 4326),
    location_updated_at TIMESTAMPTZ
);

CREATE INDEX IF NOT EXISTS users_location_gix ON users USING GIST (location);

SELECT cron.schedule(
    'delete-stale-users',
    '*/15 * * * *',
    $$
        DELETE FROM users
        WHERE location_updated_at < now() - interval '15 minutes';
    $$
);
