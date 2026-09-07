# The sync server as one small image. Build from the repository root:
#
#     docker build -t yanuka-server -f server/Dockerfile .
#     docker run -d -p 8787:8787 -v otzar-data:/data yanuka-server
#
# The first `docker logs` shows the pairing code. Add
# `--build-arg FEATURES=sqlcipher` and set YANUKA_DB_KEY to encrypt the
# server's copy at rest (slower build: OpenSSL is compiled in).

FROM rust:1-bookworm AS build
ARG FEATURES=""
WORKDIR /src
# The whole workspace manifest set, so Cargo.lock stays authoritative. Only
# the server and the two storage crates are compiled; the desktop shell's
# manifest is read for the workspace graph and nothing of Tauri is built.
COPY Cargo.toml Cargo.lock ./
COPY crates ./crates
COPY server ./server
COPY apps/desktop/src-tauri/Cargo.toml ./apps/desktop/src-tauri/Cargo.toml
COPY apps/desktop/src-tauri/src ./apps/desktop/src-tauri/src
COPY packages/database/migrations ./packages/database/migrations
COPY packages/database/seeds ./packages/database/seeds
RUN cargo build --release --locked -p yanuka-server ${FEATURES:+--features ${FEATURES}}

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates \
 && rm -rf /var/lib/apt/lists/*
COPY --from=build /src/target/release/yanuka-server /usr/local/bin/yanuka-server
ENV YANUKA_DATA_DIR=/data YANUKA_BIND=0.0.0.0:8787
VOLUME /data
EXPOSE 8787
ENTRYPOINT ["yanuka-server"]
CMD ["serve"]
