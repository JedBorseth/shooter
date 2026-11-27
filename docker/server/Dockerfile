# The headless dedicated server: the "Linux Server" Godot export and the Rust
# game extension.
#
# The exported server comes in as the named build context `server`: a
# directory holding what CI zips as shooter-<build>-server-linux-x86_64.zip.
# From the repository root:
#
#   docker build --build-context server=builds/server -t shooter-server docker/server
#
# See docs/server.md for running it.

# Debian 13 ships glibc 2.41; the binaries need 2.39 or newer (ci.yml).
FROM debian:trixie-slim

# tini runs as PID 1 and passes `docker stop`'s SIGTERM on: Godot installs no
# SIGTERM handler, and as PID 1 it would ignore the signal until killed.
# Godot's headless export loads nothing else from the system, but it needs a
# writable home for its user data (logs).
RUN apt-get update \
    && apt-get install -y --no-install-recommends tini \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --system --create-home --home-dir /home/shooter --uid 10001 shooter

WORKDIR /opt/shooter
COPY --from=server --chmod=755 shooter-server.x86_64 ./
COPY --from=server --chmod=644 shooter-server.pck libshooter_game.so ./
COPY --chmod=755 entrypoint.sh /usr/local/bin/shooter-entrypoint

USER shooter
ENV SHOOTER_PORT=27015 \
    SHOOTER_MAP=breakwater \
    SHOOTER_MODE=tdm
EXPOSE 27015/udp 27016/udp

# The query port answers once the map has loaded and the server listens.
HEALTHCHECK --interval=10s --timeout=4s --start-period=60s --retries=3 \
    CMD ["/usr/local/bin/shooter-entrypoint", "--health"]

ENTRYPOINT ["/usr/bin/tini", "--", "/usr/local/bin/shooter-entrypoint"]

ARG BUILD=dev
LABEL org.opencontainers.image.title="Shooter dedicated server" \
      org.opencontainers.image.source="https://github.com/JedBorseth/shooter" \
      org.opencontainers.image.version="${BUILD}"
