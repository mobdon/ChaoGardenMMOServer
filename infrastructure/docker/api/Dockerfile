FROM rust:1-slim AS build
WORKDIR /src
# Copy manifests first so dependency compilation is cached independently of
# application code changes.
COPY Cargo.toml Cargo.lock ./
COPY crates ./crates
COPY src ./src
RUN cargo build --release --locked --bin chao-game-server

FROM debian:12-slim AS runtime
# Non-root: dedicated uid/gid, no login shell, minimal image surface.
RUN groupadd -r -g 10001 chao && useradd -r -u 10001 -g chao -s /usr/sbin/nologin chao \
    && mkdir -p /data && chown chao:chao /data
COPY --from=build /src/target/release/chao-game-server /usr/local/bin/chao-game-server
USER 10001:10001
ENV CHAO_DATA_DIR=/data \
    GAME_HOST=0.0.0.0 \
    GAME_PORT=3001
WORKDIR /data
EXPOSE 3001
# Platform orchestrators (Fly) probe /livez and /readyz directly; a container
# HEALTHCHECK would need curl in the image, which we deliberately omit.
ENTRYPOINT ["/usr/local/bin/chao-game-server"]
