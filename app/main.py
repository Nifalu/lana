from contextlib import asynccontextmanager
from typing import Annotated

from fastapi import Depends, FastAPI, Request
from psycopg import AsyncConnection
from psycopg_pool import AsyncConnectionPool

# from initdb import init_db
from .models import LocationIN, LocationOUT, NeighborhoodResponse


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


@app.post("/location/")
def upload_location(device_id: str, location: LocationIN):
    locout = LocationOUT(longitude=location.longitude, latitude=location.latitude)
    return {"dev_id": device_id, "in": location, "out": locout}


@app.get("/quartiere", response_model=NeighborhoodResponse)
async def get_neighborhoods(conn: DbConnection):
    async with conn.cursor() as cur:
        await cur.execute("SELECT wov_name FROM basel_neighborhoods;")
        rows = await cur.fetchall()

    neighborhoods = [row[0] for row in rows]
    return NeighborhoodResponse(
        neighborhoods=neighborhoods,
    )
