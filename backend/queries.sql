-- Example Table Creation including GIST index
CREATE TABLE users (
    id          bigint PRIMARY KEY,
    name        text,
    location    geography(Point, 4326),
    location_updated_at TIMESTAMPTZ
);

CREATE INDEX users_location_gix
ON users
USING GIST (location);

-- Example Value important longitude first then latitude
INSERT INTO users (id, name, location, location_updated_at)
VALUES (
    123,
    'Alice',
    ST_SetSRID(ST_MakePoint(8.5417, 47.3769), 4326)::geography,
    now()
);

-- Find within radius and order by distance
SELECT
    u.id,
    u.name,
    ST_Distance(sender.location, u.location) AS distance_m
FROM users sender
JOIN users u
  ON ST_DWithin(sender.location, u.location, 10000)
WHERE sender.id = 123
  AND u.id <> sender.id
ORDER BY distance_m;


-- Anon sender
SELECT
    u.id,
    ST_Distance(sender.location, u.location) AS distance_m,
    u.location_updated_at
    FROM users u
CROSS JOIN (
    SELECT ST_SetSRID(
        ST_MakePoint(7.5896, 47.5670),
        4326
    )::geography AS location
) sender
WHERE ST_DWithin(
      sender.location,
      u.location,
      500
  )
ORDER BY
    u.location_updated_at DESC,
    distance_m ASC;



-- update location
DELETE FROM users
WHERE location_updated_at < now() - interval '15 minutes';

-- sql cron job
SELECT cron.schedule(
    'delete-stale-users',
    '*/15 * * * *',
    $$
        DELETE FROM users
        WHERE location_updated_at < now() - interval '15 minutes';
    $$
);

-- Drop force
DROP DATABASE hackamrhein WITH (FORCE);

-- Create DB
CREATE DATABASE hackamrhein;
