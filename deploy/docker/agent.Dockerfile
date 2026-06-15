# Multi-stage build for celerex-api. The build context is a STAGED dir assembled
# by build-image.sh that preserves the sibling layout the Cargo path deps expect:
#   <context>/Celerex/...                              (the workspace)
#   <context>/lucidity/Galvo/engine/galvo-federation   (cross-repo path dep)
FROM rust:1-bookworm AS builder
WORKDIR /build
COPY Celerex/ Celerex/
COPY lucidity/ lucidity/
WORKDIR /build/Celerex
RUN cargo build -p celerex-api --release

FROM debian:bookworm-slim AS runtime
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*
COPY --from=builder /build/Celerex/target/release/celerex-api /usr/local/bin/celerex-api
EXPOSE 3001
CMD ["celerex-api"]
