#!/usr/bin/env bash
# Publishes dist/ as the rolling `nightly` pre-release, which the Shooter
# launcher follows. Zips are uploaded first and latest.json last, so a launcher
# never reads a manifest whose files are missing. The previous build's zips
# stay one more round for launchers that read the old manifest a moment ago.
#
# Usage: publish-nightly.sh <dist dir>   (needs GH_TOKEN with contents: write)
set -euo pipefail

dist=$1
tag=nightly
build=$(jq -r .build "$dist/latest.json")

if ! gh release view "$tag" >/dev/null 2>&1; then
  gh release create "$tag" --prerelease --target "$GITHUB_SHA" --title "Nightly" \
    --notes "Created by the Build workflow."
fi

previous=$(gh release download "$tag" --pattern latest.json --output - 2>/dev/null |
  jq -r '.builds[].file' 2>/dev/null || true)

gh release upload "$tag" "$dist"/*.zip "$dist"/shooter-launcher-* --clobber
gh release upload "$tag" "$dist/latest.json" --clobber

# Point the tag (and the release's source archives) at this commit.
gh api --method PATCH "repos/$GITHUB_REPOSITORY/git/refs/tags/$tag" \
  -f sha="$GITHUB_SHA" -F force=true >/dev/null
gh release edit "$tag" --prerelease --title "Nightly ($build)" --notes "$(cat <<NOTES
The latest build of \`main\`: $GITHUB_SHA, published $(jq -r .published "$dist/latest.json").

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
