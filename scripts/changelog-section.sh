#!/usr/bin/env bash
# Print the body of one version's section from CHANGELOG.md (without the
# "## [x.y.z]" heading, the link-reference lines, or surrounding blank lines).
# Used by the release workflow to fill in the GitHub release description.
#
# Usage: scripts/changelog-section.sh <version> [changelog]
#   e.g. scripts/changelog-section.sh 0.0.3
# Exits 1 if the version has no section, or the section is empty.
set -euo pipefail

version="${1:?usage: $0 <version> [changelog]}"
changelog="${2:-CHANGELOG.md}"

section="$(
  awk -v ver="$version" '
    /^## \[/ { if (found) exit; if (index($0, "## [" ver "]") == 1) { found = 1; next } }
    found && !/^\[[^]]+\]: / { print }
  ' "$changelog" |
    sed -e '/./,$!d' |
    sed -e ':a' -e '/^\n*$/{$d;N;ba' -e '}'
)"

if [[ -z "$section" ]]; then
  echo "no '## [$version]' section in $changelog" >&2
  exit 1
fi

printf '%s\n' "$section"
