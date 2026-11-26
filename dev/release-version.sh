#!/usr/bin/env bash
# Compute the version from the same history for a PR merge tree and the main merge.
set -euo pipefail

event=${1:?event}
tags=$(git tag --list 'v[0-9]*')
matches=()
while IFS= read -r tag; do
    if [[ $tag =~ ^v(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$ ]]; then
        matches+=(--match "$tag")
    fi
done <<< "$tags"
last=
if (( ${#matches[@]} )); then
    last=$(git describe --tags --first-parent --abbrev=0 "${matches[@]}" HEAD 2>/dev/null) || last=
fi
log=$(git log --no-merges --format=%s "${last:+$last..}HEAD")
IFS=. read -r major minor patch <<< "${last#v}"
major=${major:-0}
minor=${minor:-0}
patch=${patch:-0}

if grep -qE '^[a-z]+(\([^)]*\))?!:' <<< "$log"; then
    major=$((major + 1)); minor=0; patch=0
elif grep -qE '^feat(\([^)]*\))?:' <<< "$log"; then
    minor=$((minor + 1)); patch=0
elif grep -qE '^(fix|perf)(\([^)]*\))?:' <<< "$log" || [[ $event != push ]]; then
    patch=$((patch + 1))
else
    echo "No release commits since ${last:-the start of the repository}."
    exit 0
fi

printf 'tag=v%s.%s.%s\nlast=%s\n' "$major" "$minor" "$patch" "$last" >> "${GITHUB_OUTPUT:?output file}"
