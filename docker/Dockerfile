FROM postgres:17

USER root

RUN apt-get update \
    && apt-get install -y --no-install-recommends \
        postgresql-17-postgis-3 \
        postgresql-17-postgis-3-scripts \
        postgresql-17-cron \
        gdal-bin \
    && rm -rf /var/lib/apt/lists/*

RUN echo "shared_preload_libraries = 'pg_cron'" \
        >> /usr/share/postgresql/postgresql.conf.sample \
    && echo "cron.database_name = 'hackamrhein'" \
        >> /usr/share/postgresql/postgresql.conf.sample

COPY docker/init/ /docker-entrypoint-initdb.d/

COPY data/basel-neighborhoods.geojson /tmp/basel-neighborhoods.geojson

RUN chmod +x /docker-entrypoint-initdb.d/*.sh

USER postgres
