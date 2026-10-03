from contextlib import asynccontextmanager
from datetime import datetime as dt
from typing import Annotated, Literal

import fastapi
import httpx
import pytz
from fastapi import Depends, FastAPI, HTTPException, Request
from psycopg import AsyncConnection
from psycopg.rows import dict_row
from psycopg_pool import AsyncConnectionPool

# from initdb import init_db
from .models import LocationIN, LocationOUT
from .utils import decode_valhalla_shape


async def get_db(request: Request):
    async with request.app.state.db.connection() as conn:
        yield conn


DbConnection = Annotated[AsyncConnection, Depends(get_db)]


@asynccontextmanager
async def lifespan(app: FastAPI):
    pool = AsyncConnectionPool(
        conninfo=(
            "host=localhost "
            "port=5432 "
            "dbname=hackamrhein "
            "user=postgres "
            "password=hackzheworld"
        ),
        min_size=2,
        max_size=10,
        open=False,
    )

    await pool.open()

    try:
        # await init_db(pool)  # type: ignore
        app.state.db = pool
        yield
    finally:
        await pool.close()


app = FastAPI(lifespan=lifespan)


@app.get("/")
async def root():
    return {"message": "Hello Bigger Applications!"}


@app.post("/location")
async def upsert_location(conn: DbConnection, device_id: str, location: LocationIN):
    loc_with_time = LocationOUT(
        longitude=location.longitude,
        latitude=location.latitude,
        location_updated_at=dt.now(tz=pytz.timezone("Europe/Zurich")).isoformat(),
    )
    async with conn.cursor() as cur:
        await cur.execute(
            """
                INSERT INTO users (
                    id,
                    location,
                    location_updated_at
                )
                VALUES (
                    %s,
                    ST_SetSRID(
                        ST_MakePoint(%s, %s),
                        4326
                    )::geography,
                    %s
                )
                ON CONFLICT (id)
                DO UPDATE SET
                    location = EXCLUDED.location,
                    location_updated_at = EXCLUDED.location_updated_at
                """,
            (
                device_id,
                loc_with_time.longitude,
                loc_with_time.latitude,
                loc_with_time.location_updated_at,
            ),
        )
        await conn.commit()
    return fastapi.Response(status_code=200)


@app.post("/sos")
async def sos(conn: DbConnection, location: LocationIN):
    pass


@app.post("/get_closest_helpers")
async def get_closest_helpers(conn: DbConnection, sos_location: LocationIN):
    async with conn.cursor() as cur:
        await cur.execute(
            """
               SELECT
                   u.id,
                   ST_Distance(sender.location, u.location) AS distance_m,
                   u.location_updated_at,
                   now() - u.location_updated_at AS location_age
               FROM users u
               CROSS JOIN (
                   SELECT ST_SetSRID(
                       ST_MakePoint(%s, %s),
                       4326
                   )::geography AS location
               ) sender
               WHERE u.location_updated_at >= now() - interval '15 minutes'
                 AND ST_DWithin(
                     sender.location,
                     u.location,
                     500
                 )
               ORDER BY
                   distance_m ASC,
                   u.location_updated_at DESC;
               """,
            (sos_location.longitude, sos_location.latitude),
        )
        res = await cur.fetchall()
        ret_res = [
            {
                "id": u[0],
                "distance_m": u[1],
                "location_updated_at": u[2],
                "location_age": u[3],
            }
            for u in res
        ]
        return ret_res


"""
@app.get("/quartiere", response_model=NeighborhoodResponse)
async def get_neighborhoods(conn: DbConnection):
    async with conn.cursor() as cur:
        await cur.execute("SELECT wov_name FROM basel_neighborhoods;")
        rows = await cur.fetchall()

    neighborhoods = [row[0] for row in rows]
    return NeighborhoodResponse(
        neighborhoods=neighborhoods,
    )
"""

LocationType = Literal["fountain", "cool_place", "all"]


@app.post("/route_to_closest_cooling")
async def get_route_closest_cooling(
    conn: DbConnection,
    location: LocationIN,
    filter: list[LocationType] | None = None,
    number_results: int = 1,
):
    TABLES = {
        "fountain": "fountains",
        "cool_place": "cool_places",
    }
    if filter is None or "all" in filter or len(filter) == 0:
        selected = list(TABLES.keys())
    else:
        selected = filter

    queries = []

    for location_type in selected:
        table = TABLES[location_type]

        queries.append(
            f"""
            SELECT
                id,
                name,
                '{location_type}' AS type,
                ST_X(location::geometry) AS longitude,
                ST_Y(location::geometry) AS latitude,
                ST_Distance(
                    location,
                    ST_SetSRID(
                        ST_MakePoint(%s, %s),
                        4326
                    )::geography
                ) AS distance
            FROM {table}
            """
        )

    sql = f"""
        SELECT *
        FROM (
            {" UNION ALL ".join(queries)}
        ) AS locations
        ORDER BY distance
        LIMIT {number_results};
    """

    params = []
    for _ in selected:
        params.extend([location.longitude, location.latitude])

    async with conn.cursor(row_factory=dict_row) as cur:
        await cur.execute(sql, params)
        result = await cur.fetchall()

    if result is None:
        raise HTTPException(status_code=404, detail="No cooling location found")

    final_res = []
    for res in result:
        response = await _get_route(
            start=location,
            end=LocationIN(
                longitude=res["longitude"],
                latitude=res["latitude"],
            ),
        )
        data = response.json()
        shape = data.get("trip").get("legs")[0].get("shape")
        coordinates = decode_valhalla_shape(shape)
        final_res.append(
            {
                **res,
                "type": "Feature",
                "properties": {
                    "length_km": data["trip"]["summary"]["length"],
                    "time_seconds": data["trip"]["summary"]["time"],
                },
                "geometry": {
                    "type": "LineString",
                    "coordinates": coordinates,
                },
            }
        )
    return final_res


async def _get_route(start: LocationIN, end: LocationIN):
    payload = {
        "locations": [
            {
                "lat": start.latitude,
                "lon": start.longitude,
                "type": "break",
            },
            {
                "lat": end.latitude,
                "lon": end.longitude,
                "type": "break",
            },
        ],
        "costing": "pedestrian",
        "units": "kilometers",
        "shape_format": "geojson",
    }
    async with httpx.AsyncClient() as client:
        response = await client.post(
            "http://localhost:8080/route",
            json=payload,
        )
    return response


# TODO: adjust to proper url once on switchcloud!
"""
{
  "start": {
    "longitude": 7.5896,
    "latitude": 47.5476
  },
  "end": {
    "longitude": 7.6075,
    "latitude": 47.5670
  }
}
"""


@app.post("/calculate_route")
async def get_route(start: LocationIN, end: LocationIN, verbose: bool = False):
    response = await _get_route(start=start, end=end)

    response.raise_for_status()
    if not verbose:
        data = response.json()
        shape = data.get("trip").get("legs")[0].get("shape")
        coordinates = decode_valhalla_shape(shape)
        return {
            "type": "Feature",
            "properties": {
                "length_km": data["trip"]["summary"]["length"],
                "time_seconds": data["trip"]["summary"]["time"],
            },
            "geometry": {
                "type": "LineString",
                "coordinates": coordinates,
            },
        }
    return response.json()
