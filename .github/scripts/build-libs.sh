#!/usr/bin/env bash
# Builds the game library and the launcher for one platform into ./out. Run from
# the repository root.
#
# Usage: build-libs.sh linux|windows|macos
# CARGO_PROFILE picks the Cargo profile (default: release).
set -euo pipefail

platform=$1
profile=${CARGO_PROFILE:-release}
out=$PWD/out
mkdir -p "$out"
cd rust

case "$platform" in
  linux)
    cargo build --locked --profile "$profile" -p shooter-game -p shooter-launcher
    cp "target/$profile/libshooter_game.so" "$out/"
    cp "target/$profile/shooter-launcher" "$out/shooter-launcher-linux-x86_64"
    # Fail if a binary needs a glibc newer than 2.39, the baseline the Linux job's
    # ubuntu-24.04 image ships (see ci.yml).
    newest=$(objdump -T "$out/libshooter_game.so" "$out/shooter-launcher-linux-x86_64" |
      grep -o 'GLIBC_[0-9.]*' | sort -u -V | tail -n 1)
    echo "Newest glibc symbol: $newest"
    if [[ "$(printf '%s\n' "$newest" GLIBC_2.39 | sort -V | tail -n 1)" != GLIBC_2.39 ]]; then
      echo "::error::Linux binaries need $newest; the baseline is GLIBC_2.39" >&2
      exit 1
    fi
    ;;
  windows)
    cargo build --locked --profile "$profile" -p shooter-game -p shooter-launcher
    cp "target/$profile/shooter_game.dll" "$out/"
    cp "target/$profile/shooter-launcher.exe" "$out/shooter-launcher-windows-x86_64.exe"
    ;;
  macos)
    # One cargo run builds both architectures in parallel.
    cargo build --locked --profile "$profile" -p shooter-game -p shooter-launcher \
      --target aarch64-apple-darwin --target x86_64-apple-darwin
    lipo -create -output "$out/libshooter_game.dylib" \
      "target/aarch64-apple-darwin/$profile/libshooter_game.dylib" \
      "target/x86_64-apple-darwin/$profile/libshooter_game.dylib"
    lipo -create -output "$out/shooter-launcher-macos-universal" \
      "target/aarch64-apple-darwin/$profile/shooter-launcher" \
      "target/x86_64-apple-darwin/$profile/shooter-launcher"
    codesign --force --sign - "$out/shooter-launcher-macos-universal"
    ;;
  *)
    echo "Unknown platform: $platform" >&2
    exit 1
    ;;
esac
ls -l "$out"
