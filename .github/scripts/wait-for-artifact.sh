#!/usr/bin/env bash
# Waits until another job in this workflow run uploads an artifact, then
# downloads it. The export job uses this to start preparing Godot while the
# libraries are still building, instead of waiting for every library job to end.
#
# Usage: wait-for-artifact.sh <artifact> <job name> <directory>
# Fails as soon as <job name> finishes without success. Needs GH_TOKEN with
# actions: read.
set -euo pipefail

artifact=$1
job=$2
dest=$3
repo=$GITHUB_REPOSITORY
run=$GITHUB_RUN_ID
started=$(date +%s)
polls=0

while true; do
  count=$(gh api "repos/$repo/actions/runs/$run/artifacts?name=$artifact" -q '.total_count' || echo 0)
  if [[ $count != 0 ]]; then
    # The artifact is listed as soon as its upload starts, before GitHub has
    # finalized it; a download then finds "no valid artifacts". Retry a few
    # times before giving up.
    for try in 1 2 3 4 5 6; do
      if gh run download "$run" -R "$repo" -n "$artifact" -D "$dest"; then
        echo "Got $artifact after $(($(date +%s) - started)) s."
        exit 0
      fi
      echo "$artifact is listed but not downloadable yet (try $try); retrying."
      sleep 10
    done
    echo "::error::$artifact is listed but could not be downloaded."
    exit 1
  fi
  # Checking the job costs another API call, so do it every few polls. The
  # GITHUB_TOKEN allows 1,000 calls an hour across the whole repository.
  if ((polls % 3 == 2)); then
    conclusion=$(gh api --paginate "repos/$repo/actions/runs/$run/attempts/$GITHUB_RUN_ATTEMPT/jobs" \
      -q ".jobs[] | select(.name == \"$job\") | .conclusion // empty" || true)
    if [[ -n $conclusion && $conclusion != success ]]; then
      # A job can upload its library and fail a later step; the check above
      # already took the artifact in that case.
      echo "::error::$job finished ($conclusion) without uploading $artifact."
      exit 1
    fi
  fi
  polls=$((polls + 1))
  sleep 8
done
