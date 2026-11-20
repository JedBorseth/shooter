#!/usr/bin/env bash
# Builds the dedicated server's Docker image from an exported server directory,
# starts it, and waits for its health check (the query port answering). With
# PUSH set, it then pushes the image to the GitHub Container Registry.
#
# Usage: server-image.sh <server dir> <build name>
#   PUSH=nightly   push :nightly and :<build> (main)
#   PUSH=release   push :latest and :<build> (v* tags)
# Pushing needs `docker login ghcr.io` first (the workflow's GITHUB_TOKEN with
# packages: write). See docs/server.md.
set -euo pipefail

server_dir=$1
build=$2
image=ghcr.io/${GITHUB_REPOSITORY:-jedborseth/shooter}-server
image=${image,,}

docker build --build-context "server=$server_dir" --build-arg "BUILD=$build" \
  -t "$image:$build" docker/server
docker image ls "$image"

# The image must start a server that answers its query port.
name=shooter-server-check
docker rm -f "$name" >/dev/null 2>&1 || true
docker run -d --name "$name" -e SHOOTER_BOTS=2 -e SHOOTER_BOT_SKILL=normal "$image:$build" >/dev/null
status=starting
for _ in $(seq 1 90); do
  status=$(docker inspect -f '{{.State.Health.Status}}' "$name" 2>/dev/null || echo exited)
  [[ "$(docker inspect -f '{{.State.Running}}' "$name")" == true ]] || status=exited
  [[ "$status" == starting ]] || break
  sleep 2
done
docker logs "$name" 2>&1 | tail -n 20
docker exec "$name" shooter-entrypoint --health || true
docker rm -f "$name" >/dev/null
if [[ "$status" != healthy ]]; then
  echo "::error::The server image did not become healthy ($status)" >&2
  exit 1
fi
echo "The server image is healthy."

case "${PUSH:-}" in
  nightly) tags=(nightly "$build") ;;
  release) tags=(latest "$build") ;;
  "") exit 0 ;;
  *) echo "Unknown PUSH=$PUSH" >&2; exit 1 ;;
esac
for tag in "${tags[@]}"; do
  [[ "$tag" == "$build" ]] || docker tag "$image:$build" "$image:$tag"
  docker push "$image:$tag"
done
