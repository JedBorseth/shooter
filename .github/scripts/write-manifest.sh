#!/usr/bin/env bash
# Writes dist/latest.json: the build's commit, protocol version, and the name,
# size, and SHA-256 of every zip and launcher in dist/. The Shooter launcher
# reads it to find the newest build (rust/launcher/src/manifest.rs).
#
# Usage: write-manifest.sh <dist dir> <build name prefix, e.g. shooter-aebe7ae>
set -euo pipefail

dist=$1
name=$2
build=${name#shooter-}
protocol=$(sed -n 's/^pub const PROTOCOL_VERSION: u16 = \([0-9]*\);$/\1/p' rust/sim/src/net/protocol.rs)
[[ -n "$protocol" ]] || { echo "PROTOCOL_VERSION not found" >&2; exit 1; }

asset() {
  local file=$1
  jq -n --arg file "$(basename "$file")" \
    --argjson size "$(stat -c %s "$file")" \
    --arg sha256 "$(sha256sum "$file" | cut -d' ' -f1)" \
    '{file: $file, size: $size, sha256: $sha256}'
}

build_entry() {
  local platform=$1 exe=$2 zip="$dist/$name-$1.zip"
  grep -xF "$exe" <<<"$(unzip -Z1 "$zip")" >/dev/null || { echo "$zip has no $exe" >&2; exit 1; }
  asset "$zip" | jq --arg platform "$platform" --arg exe "$exe" '{($platform): (. + {exe: $exe})}'
}

mac_files=$(unzip -Z1 "$dist/$name-macos-universal.zip")
mac_exe=$(grep -E '^[^/]+\.app/Contents/MacOS/[^/]+$' <<<"$mac_files" | sed -n 1p)

builds=$(
  {
    build_entry linux-x86_64 shooter.x86_64
    build_entry windows-x86_64 shooter.exe
    build_entry macos-universal "$mac_exe"
    build_entry server-linux-x86_64 shooter-server.x86_64
  } | jq -s 'add'
)

launchers=$(
  for platform in linux-x86_64 windows-x86_64 macos-universal; do
    file=$(ls "$dist"/shooter-launcher-"$platform"*)
    asset "$file" | jq --arg platform "$platform" '{($platform): .}'
  done | jq -s 'add'
)

jq -n \
  --arg build "$build" \
  --arg commit "${GITHUB_SHA:-$(git rev-parse HEAD)}" \
  --argjson protocol "$protocol" \
  --arg published "$(date -u +%Y-%m-%dT%H:%M:%SZ)" \
  --argjson builds "$builds" \
  --argjson launchers "$launchers" \
  '{schema: 1, build: $build, commit: $commit, protocol: $protocol,
    published: $published, builds: $builds, launchers: $launchers}' \
  > "$dist/latest.json"
cat "$dist/latest.json"
