#!/bin/bash

set -e

echo "========================================"
echo "Creating PostGIS extension"
echo "========================================"

psql \
    -v ON_ERROR_STOP=1 \
    --username "$POSTGRES_USER" \
    --dbname "$POSTGRES_DB" \
    -c "CREATE EXTENSION IF NOT EXISTS postgis; CREATE EXTENSION IF NOT EXISTS pg_cron;"

echo "PostGIS enabled."

echo "========================================"
echo "Importing Basel neighborhoods"
echo "========================================"

ogr2ogr \
    -f PostgreSQL \
    "PG:dbname=${POSTGRES_DB} user=${POSTGRES_USER} password=${POSTGRES_PASSWORD}" \
    /data/basel-neighborhoods.geojson \
    -nln basel_neighborhoods \
    -nlt PROMOTE_TO_MULTI \
    -lco GEOMETRY_NAME=geom \
    -lco FID=id

echo "Basel neighborhoods imported successfully."

echo "========================================"
echo "Importing fountains"
echo "========================================"

python3 /docker-entrypoint-initdb.d/init.py /data/fountains.json /data/cool_places.json

echo "Fountains imported successfully."
