# syntax=docker/dockerfile:1
# Multi-stage build for celerex-api. The build context is a STAGED dir assembled
# by build-image.sh that preserves the sibling layout the Cargo path deps expect:
#   <context>/Celerex/...                              (the workspace)
#   <context>/lucidity/Galvo/engine/...                (cross-repo path deps)
#
# Local/dev image: a DEBUG build (no --release) — far faster to compile and fine
# for a local cluster. Cloud overlays can build --release later. BuildKit cache
# mounts keep the cargo registry + target across builds so reruns are fast; the
# binary is copied OUT of the cache-mounted target (COPY --from can't read a mount).
FROM rust:1-bookworm AS builder
WORKDIR /build
COPY Celerex/ Celerex/
COPY lucidity/ lucidity/
WORKDIR /build/Celerex
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/build/Celerex/target \
    cargo build -p celerex-api && \
    cp target/debug/celerex-api /celerex-api

FROM debian:bookworm-slim AS runtime
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*
COPY --from=builder /celerex-api /usr/local/bin/celerex-api
EXPOSE 3001
CMD ["celerex-api"]
