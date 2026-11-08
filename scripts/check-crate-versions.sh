#!/usr/bin/env bash
# Fail when a crate's published contents changed after its current version was
# released.
#
# publish.yml tags each published crate as <crate dir>-v<version>. If that tag
# exists, crates.io already has this version, so any change since the tag would
# never reach crates.io and would break dependents that build against the
# registry. Bump the version instead. Two kinds of change count:
#   - the crate's src/ or build.rs;
#   - its resolved dependencies or features, including requirements inherited
#     from [workspace.dependencies] in the root Cargo.toml. Dev-dependencies are
#     ignored because they do not reach the published crate.
set -euo pipefail

cd "$(dirname "$0")/.."

# The parts of a package's metadata that reach crates.io, in a stable order.
published_shape='{
  dependencies: [.dependencies[] | select(.kind != "dev")
    | {name, rename, req, kind, target, optional, uses_default_features, features: (.features | sort)}]
    | sort_by(.name, .kind // "", .target // ""),
  features: (.features | with_entries(.value |= sort))
}'

metadata() { cargo metadata --no-deps --offline --format-version 1 --manifest-path "$1/Cargo.toml"; }

head_metadata=$(metadata .)
scratch=$(mktemp -d)
trap 'rm -rf "$scratch"' EXIT

# Metadata of the whole workspace as of a tag, extracted once per tag.
tag_metadata() {
  local tag=$1 out="$scratch/$1.json"
  if [ ! -f "$out" ]; then
    mkdir -p "$scratch/$tag"
    git archive "$tag" | tar -x -C "$scratch/$tag"
    metadata "$scratch/$tag" >"$out"
  fi
  cat "$out"
}

status=0
while read -r manifest name version; do
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

  shape=".packages[] | select(.name == \$name) | $published_shape"
  before=$(tag_metadata "$tag" | jq -S --arg name "$name" "$shape")
  after=$(jq -S --arg name "$name" "$shape" <<<"$head_metadata")
  if [ "$before" != "$after" ]; then
    echo "::error file=crates/${crate}/Cargo.toml::${crate} ${version} is already published (${tag}) but its dependencies or features changed; bump its version."
    diff <(echo "$before") <(echo "$after") | grep '^[<>]' | sed 's/^/  /' || true
    status=1
  fi
done < <(jq -r '.packages[] | select(.publish != []) | "\(.manifest_path) \(.name) \(.version)"' <<<"$head_metadata")

[ "$status" -eq 0 ] && echo "Every published crate version matches its released code and dependencies."
exit "$status"
