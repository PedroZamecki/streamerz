#!/usr/bin/env bash
# Find a successful build of the merged PR with both release artifacts available.
set -euo pipefail

want=${1:?version and tree}
repo=${GH_REPO:?repository}
# The protected branch uses merge commits. A direct push has no PR parent.
if ! head=$(git rev-parse --verify --quiet HEAD^2); then
    exit 0
fi
runs=$(gh api --paginate "repos/$repo/actions/workflows/build.yml/runs?event=pull_request&status=success&head_sha=$head&per_page=100" \
          --jq ".workflow_runs[] | select(.head_repository.full_name == \"$repo\") | .id")
for id in $runs; do
    names=$(gh api --paginate "repos/$repo/actions/runs/$id/artifacts?per_page=100" \
               --jq '.artifacts[] | select(.expired == false) | .name')
    if grep -Fqx "streamerz-windows-$want" <<< "$names" && grep -Fqx "streamerz-linux-$want" <<< "$names"; then
        echo "Reusing the binaries of run $id."
        printf 'run=%s\n' "$id" >> "${GITHUB_OUTPUT:?output file}"
        break
    fi
done
