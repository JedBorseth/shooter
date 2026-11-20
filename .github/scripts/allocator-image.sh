#!/usr/bin/env bash
# Builds the server allocator's image (docker/allocator) from the rust/
# workspace. With PUSH set, pushes it to the GitHub Container Registry like
# server-image.sh: PUSH=nightly pushes :nightly and :<build>, PUSH=release
# :latest and :<build>.
#
# Usage: allocator-image.sh <build name>
set -euo pipefail

build=$1
image=ghcr.io/${GITHUB_REPOSITORY:-jedborseth/shooter}-allocator
image=${image,,}

docker build -f docker/allocator/Dockerfile --build-arg "BUILD=$build" -t "$image:$build" rust
docker image ls "$image"

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
