#!/bin/bash

set -e

echo "========================================"
echo "Importing Basel neighborhoods"
echo "========================================"

ogr2ogr \
    -f PostgreSQL \
    "PG:dbname=${POSTGRES_DB} user=${POSTGRES_USER} password=${POSTGRES_PASSWORD}" \
    /tmp/basel-neighborhoods.geojson \
    -nln basel_neighborhoods \
    -nlt PROMOTE_TO_MULTI \
    -lco GEOMETRY_NAME=geom \
    -lco FID=id

echo "Basel neighborhoods imported successfully."
