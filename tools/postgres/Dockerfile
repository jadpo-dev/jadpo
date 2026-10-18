# The PostgreSQL CI's tests run against (docs/compiler-runtime.md §2.7):
# PostgreSQL 16 with pgvector, which stores and searches vectors (area 21),
# and PostGIS, which stores geometries (area 20), each pinned. Chosen in
# chunk 5.5v1: the official image on Debian bookworm, pinned by digest, with
# both extensions from the PostgreSQL project's own packages at exact
# versions. `postgis/postgis:16-3.4` and its later tags are still built on
# Debian bullseye, whose support has ended, so nothing is installed on them.
#
# An old pinned version leaves apt.postgresql.org's index after a few
# releases, and the build then fails naming it: bump it here.
FROM postgres:16.14-bookworm@sha256:64154d0babcb1741988719e703419af0382b19953706149f9872fbd0f438efa8

ARG PGVECTOR_VERSION=0.8.7-1.pgdg12+1
ARG POSTGIS_VERSION=3.6.4+dfsg-2.pgdg12+1

RUN set -eux; \
    apt-get update; \
    apt-get install -y --no-install-recommends \
        "postgresql-16-pgvector=${PGVECTOR_VERSION}" \
        "postgresql-16-postgis-3=${POSTGIS_VERSION}" \
        "postgresql-16-postgis-3-scripts=${POSTGIS_VERSION}"; \
    rm -rf /var/lib/apt/lists/*
