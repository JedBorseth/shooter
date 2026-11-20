# The server allocator (rust/allocator), as a static binary in an empty image.
# Build context: the rust/ workspace. Docker Compose builds it
# (docker/online/compose.yml); by hand, from the repository root:
#
#   docker build -f docker/allocator/Dockerfile -t shooter-allocator rust

FROM rust:1-alpine AS build
RUN apk add --no-cache musl-dev
WORKDIR /src
COPY . .
RUN cargo build --release --locked -p shooter-allocator \
    && cp target/release/shooter-allocator /shooter-allocator

FROM scratch
ARG BUILD=dev
LABEL org.opencontainers.image.title="Shooter server allocator" \
      org.opencontainers.image.source="https://github.com/JedBorseth/shooter" \
      org.opencontainers.image.version="${BUILD}"
COPY --from=build /shooter-allocator /shooter-allocator
EXPOSE 7400
ENTRYPOINT ["/shooter-allocator"]
