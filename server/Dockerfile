# FocusTrack sync server. Build from the repository root:
#   docker build -f server/Dockerfile -t focustrack-sync .
FROM rust:1-bookworm AS build
WORKDIR /src
COPY crates/sync-proto crates/sync-proto
COPY server server
RUN cd server && cargo build --release --locked

FROM debian:bookworm-slim
RUN useradd --system --uid 10001 --home-dir /data focustrack \
    && mkdir -p /data/backups && chown -R focustrack /data
COPY --from=build /src/server/target/release/focustrack-server /usr/local/bin/focustrack-server
USER focustrack
ENV FT_SYNC_ADDR=0.0.0.0:8787 \
    FT_SYNC_DB=/data/focustrack-sync.db
EXPOSE 8787
VOLUME /data
CMD ["focustrack-server"]
