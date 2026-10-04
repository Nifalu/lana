# Lana Backend

FastAPI backend for finding nearby helpers and routing to cooling spots during heat events.

## What it does

- **Tracks live locations** (PostGIS `geography`): `POST /location`
- **Finds nearby helpers** within 500 m, updated in the last 15 min: `POST /get_closest_helpers`
- **Routes to closest cooling spots** (fountains / cool places) via Valhalla pedestrian routing: `POST /route_to_closest_cooling`
- **Calculates pedestrian routes** A → B, returns GeoJSON `LineString`: `POST /calculate_route`

Stack: `app/` (FastAPI, Python 3.13, `uv`) + `docker/` (Postgres 17 + PostGIS + pg_cron) + `routing/` (Valhalla + nginx).

Seed data on first DB start (`data/`): Basel neighborhoods, fountains, cool places. A pg_cron job deletes stale `users` (> ~15 min).

## Start services

Prerequisites: Docker, `uv`, Python 3.13.

```bash
# 1. Database (from backend/)
docker compose -f docker/docker-compose.yml up --build
# Postgres: localhost:5432, db: hackamrhein, user: postgres, password: hackzheworld

# 2. Routing (from backend/routing/, requires tiles.tar / tiles/)
docker compose up
# Routing: http://localhost:8080/route -> valhalla:8002

# 3. API (from backend/)
uv sync
uv run fastapi dev
# API: http://localhost:8000, docs: http://localhost:8000/docs
```

The API expects the DB on `localhost:5432` and routing on `localhost:8080` (see `app/main.py`).

## Examples

```bash
# Health check
curl http://localhost:8000/

# Share / update location
curl -X POST "http://localhost:8000/location?device_id=abc123" \
  -H "Content-Type: application/json" \
  -d '{"longitude": 7.5896, "latitude": 47.5476}'

# Find closest helpers near an SOS location
curl -X POST http://localhost:8000/get_closest_helpers \
  -H "Content-Type: application/json" \
  -d '{"longitude": 7.5896, "latitude": 47.5476}'

# Calculate pedestrian route A -> B
curl -X POST http://localhost:8000/calculate_route \
  -H "Content-Type: application/json" \
  -d '{"start": {"longitude": 7.5896, "latitude": 47.5476}, "end": {"longitude": 7.6075, "latitude": 47.5670}}'
```
