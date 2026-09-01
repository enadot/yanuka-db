# The sync server, as a container.
#
# Built from the repository root so the workspace manifest and lockfile resolve:
#
#     docker build -f crates/yanuka-sync-server/Dockerfile -t yanuka-sync .
#
# The runtime stage carries no toolchain and no shell utilities beyond what the
# binary links, because the one thing this container holds is other people's
# contact history — sealed, but still theirs.

FROM rust:1-bookworm AS build
WORKDIR /build

# The desktop crate is a workspace member, so its manifest has to be present for
# the workspace to resolve. It is never compiled here: `-p yanuka-sync-server`
# builds only the server's tree, which is why this image needs no webview.
COPY Cargo.toml Cargo.lock ./
COPY crates ./crates
COPY apps/desktop/src-tauri ./apps/desktop/src-tauri

RUN cargo build --release --locked -p yanuka-sync-server \
    && strip target/release/yanuka-sync-server

FROM debian:bookworm-slim

# The root certificates are copied from the build stage rather than installed.
# Nothing is fetched at runtime-image build time, so the image builds the same
# way on a machine with no package mirror reachable — and the layer is a single
# file instead of a package index.
COPY --from=build /etc/ssl/certs/ca-certificates.crt /etc/ssl/certs/ca-certificates.crt

# Unprivileged: a process that never writes to disk has no reason to be root.
RUN useradd --system --uid 10001 --no-create-home yanuka
USER yanuka

COPY --from=build /build/target/release/yanuka-sync-server /usr/local/bin/
ENV PORT=8080
EXPOSE 8080
CMD ["yanuka-sync-server"]
