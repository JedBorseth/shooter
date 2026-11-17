#!/usr/bin/env bash
# Publishes dist/ as the rolling `nightly` pre-release, which the Shooter
# launcher follows. Zips are uploaded first and latest.json last, so a launcher
# never reads a manifest whose files are missing. The previous build's zips
# stay one more round for launchers that read the old manifest a moment ago.
#
# The release and its tag are made once by hand, and the tag never moves: the
# workflow token may not create or move a tag on a commit that contains
# workflow files (that needs the `workflows` permission, which GITHUB_TOKEN
# cannot have). The release notes name the build's commit instead. To make the
# release, as a maintainer whose token has the `workflow` scope:
#   gh release create nightly --prerelease --target main --title Nightly --notes "..."
#
# Usage: publish-nightly.sh <dist dir>   (needs GH_TOKEN with contents: write)
set -euo pipefail

dist=$1
tag=nightly
build=$(jq -r .build "$dist/latest.json")

if ! gh release view "$tag" >/dev/null 2>&1; then
  echo "::error::There is no '$tag' release. Create it once by hand; see the top of $0." >&2
  exit 1
fi

previous=$(gh release download "$tag" --pattern latest.json --output - 2>/dev/null |
  jq -r '.builds[].file' 2>/dev/null || true)

gh release upload "$tag" "$dist"/*.zip "$dist"/shooter-launcher-* --clobber
gh release upload "$tag" "$dist/latest.json" --clobber

gh release edit "$tag" --prerelease --title "Nightly ($build)" --notes "$(cat <<NOTES
The latest build of \`main\`: [$build]($GITHUB_SERVER_URL/$GITHUB_REPOSITORY/commit/$GITHUB_SHA), published $(jq -r .published "$dist/latest.json").

Play it with the Shooter launcher, which downloads each new build by itself. See [Play the latest build](https://github.com/$GITHUB_REPOSITORY#play-the-latest-build).
NOTES
)"

keep=$(jq -r '.builds[].file' "$dist/latest.json"; echo "$previous")
gh release view "$tag" --json assets --jq '.assets[].name' | while read -r asset; do
  [[ "$asset" == *.zip ]] || continue
  if ! grep -qxF "$asset" <<<"$keep"; then
    echo "Removing $asset"
    gh release delete-asset "$tag" "$asset" --yes
  fi
done
