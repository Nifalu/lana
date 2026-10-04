import json
import os
import sys

import psycopg


def main():
    if len(sys.argv) != 3:
        raise SystemExit("Usage: init.py <fountains.json> <cool_places.json>")

    fountain_path = sys.argv[1]
    cool_places_path = sys.argv[2]

    with open(fountain_path, "r", encoding="utf-8") as f:
        fountains = json.load(f)

    with open(cool_places_path, "r", encoding="utf-8") as f:
        cool_places = json.load(f)

    conninfo = (
        f"dbname={os.environ['POSTGRES_DB']} "
        f"user={os.environ['POSTGRES_USER']} "
        f"password={os.environ['POSTGRES_PASSWORD']}"
    )

    with psycopg.connect(conninfo) as conn, conn.cursor() as cur:
        cur.execute("""
            CREATE TABLE IF NOT EXISTS users (
                id          varchar(255) PRIMARY KEY,
                location    geography(Point, 4326),
                location_updated_at TIMESTAMPTZ
            );
            """)
        cur.execute("""
            CREATE INDEX IF NOT EXISTS users_location_gix ON users USING GIST (location);
            """)
        cur.execute("""
            SELECT cron.schedule(
                'delete-stale-users',
                '*/15 * * * *',
                $$
                    DELETE FROM users
                    WHERE location_updated_at < now() - interval '14 minutes';
                $$
            );
            """)

    with psycopg.connect(conninfo) as conn, conn.cursor() as cur:
        cur.execute("""
                CREATE TABLE IF NOT EXISTS fountains (
                    id       bigint PRIMARY KEY,
                    name     varchar(255),
                    location geography(Point, 4326)
                );
            """)
        cur.execute("""
            CREATE INDEX IF NOT EXISTS fountain_location_gix ON users USING GIST (location);
            """)

        cur.executemany(
            """
                INSERT INTO fountains (
                    id,
                    name,
                    location
                )
                VALUES (
                    %s,
                    %s,
                    ST_SetSRID(
                        ST_MakePoint(%s, %s),
                        4326
                    )::geography
                )
                """,
            [
                (
                    fountain.get("id"),
                    fountain.get("name"),
                    fountain.get("lon"),
                    fountain.get("lat"),
                )
                for fountain in fountains
            ],
        )
        conn.commit()
    print(f"Inserted {len(fountains)} fountains.")

    with psycopg.connect(conninfo) as conn, conn.cursor() as cur:
        cur.execute("""
                CREATE TABLE IF NOT EXISTS cool_places (
                    id       bigint PRIMARY KEY,
                    name     varchar(255),
                    location geography(Point, 4326)
                );
            """)
        cur.execute("""
            CREATE INDEX IF NOT EXISTS cool_places_location_gix ON users USING GIST (location);
            """)

        cur.executemany(
            """
                INSERT INTO cool_places (
                    id,
                    name,
                    location
                )
                VALUES (
                    %s,
                    %s,
                    ST_SetSRID(
                        ST_MakePoint(%s, %s),
                        4326
                    )::geography
                )
                """,
            [
                (
                    cool_place.get("id"),
                    cool_place.get("name"),
                    cool_place.get("lon"),
                    cool_place.get("lat"),
                )
                for cool_place in cool_places
            ],
        )
        conn.commit()
    print(f"Inserted {len(cool_places)} cool places.")


if __name__ == "__main__":
    main()
