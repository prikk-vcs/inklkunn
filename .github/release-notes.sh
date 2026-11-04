#!/usr/bin/env bash
# The body of a GitHub release (release.yml; docs/RELEASING.md): the CHANGELOG section's lead — from its heading
# `## [TAG]` to the line `The entries below are in the order the work landed…` or the next `## [` — then a link to
# the whole section at the tag. A GitHub release body holds at most 125,000 characters and a section can be longer,
# so the body is the lead and never the whole list. Usage: release-notes.sh TAG REPOSITORY > release-notes.md
set -euo pipefail
tag="$1"
repo="$2"
lead="$(awk -v tag="$tag" '
  $0 ~ "^## \\[" tag "\\]" {p=1; next}
  p && /^## \[/ {exit}
  p && /^The entries below are in the order the work landed/ {exit}
  p {print}' CHANGELOG.md)"
test -n "${lead//[[:space:]]/}" || { echo "no CHANGELOG section for ${tag}" >&2; exit 1; }
printf '%s\n\nEvery change in this release, in the order the work landed: [CHANGELOG.md at %s](https://github.com/%s/blob/%s/CHANGELOG.md).\n' "$lead" "$tag" "$repo" "$tag"
