#!/usr/bin/env bash
# Fail when a crate's code changed after its current version was published.
#
# publish.yml tags each published crate as <crate dir>-v<version>. If that tag
# exists, crates.io already has this version, so any change to the crate's
# src/ or build.rs since the tag would never reach crates.io and would break
# dependents that build against the registry. Bump the version instead.
set -euo pipefail

cd "$(dirname "$0")/.."

status=0
while read -r manifest version; do
  dir=$(dirname "$manifest")
  crate=$(basename "$dir")
  tag="${crate}-v${version}"
  git rev-parse -q --verify "refs/tags/${tag}" >/dev/null || continue
  changed=$(git diff --name-only "$tag" HEAD -- "crates/${crate}/src" "crates/${crate}/build.rs")
  if [ -n "$changed" ]; then
    echo "::error file=crates/${crate}/Cargo.toml::${crate} ${version} is already published (${tag}) but its code changed; bump its version."
    echo "$changed" | sed 's/^/  /'
    status=1
  fi
done < <(cargo metadata --no-deps --format-version 1 \
  | jq -r '.packages[] | select(.publish != []) | "\(.manifest_path) \(.version)"')

[ "$status" -eq 0 ] && echo "Every published crate version matches its released code."
exit "$status"
