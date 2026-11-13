#!/bin/sh
# Install the eoka CLI from a GitHub release.
#
#   EOKA_VERSION      release to install (default: the latest eoka-cli release)
#   EOKA_INSTALL_DIR  destination directory (default: ~/.local/bin)
#   GITHUB_TOKEN      optional, raises the GitHub API rate limit
#
# An installed CLI updates itself with `eoka update`.
set -eu
repo="shrimp-software/eoka-tools"
fetch() { curl --fail --location --silent --show-error "$@"; }

version="${EOKA_VERSION:-}"
if [ -z "$version" ]; then
    # The repository's "latest" release may be an eoka-server tag, so pick the
    # highest eoka-cli-v* tag among published releases.
    # GITHUB_TOKEN is optional; it only raises the API rate limit.
    version="$(fetch ${GITHUB_TOKEN:+--header "Authorization: Bearer $GITHUB_TOKEN"} \
        "https://api.github.com/repos/$repo/releases?per_page=100" |
        awk '
            /"tag_name":/   { tag = $0; sub(/.*"tag_name": *"/, "", tag); sub(/".*/, "", tag) }
            /"draft":/      { draft = ($0 ~ /true/) }
            /"prerelease":/ {
                if (!draft && $0 !~ /true/ && tag ~ /^eoka-cli-v[0-9]+\.[0-9]+\.[0-9]+$/) {
                    sub(/^eoka-cli-v/, "", tag)
                    split(tag, part, ".")
                    rank = part[1] * 1000000 + part[2] * 1000 + part[3]
                    if (rank > best) { best = rank; latest = tag }
                }
            }
            END { print latest }')"
    test -n "$version" || { echo "Could not find the latest eoka-cli release; set EOKA_VERSION" >&2; exit 1; }
fi
case "$version" in *[!0-9.]*|'') echo "Invalid EOKA_VERSION" >&2; exit 2;; esac

case "$(uname -s)-$(uname -m)" in
    Darwin-arm64) target=aarch64-apple-darwin ;;
    Darwin-x86_64) target=x86_64-apple-darwin ;;
    Linux-aarch64|Linux-arm64) target=aarch64-unknown-linux-gnu ;;
    Linux-x86_64) target=x86_64-unknown-linux-gnu ;;
    *) echo "Unsupported platform; use cargo install eoka-cli" >&2; exit 1 ;;
esac
if command -v sha256sum >/dev/null 2>&1; then
    sha256() { sha256sum "$1" | awk '{print $1}'; }
elif command -v shasum >/dev/null 2>&1; then
    sha256() { shasum -a 256 "$1" | awk '{print $1}'; }
else
    echo "Need sha256sum or shasum to verify the download" >&2; exit 1
fi

asset="eoka-cli-${target}.tar.gz"
install_dir="${EOKA_INSTALL_DIR:-$HOME/.local/bin}"
base="https://github.com/$repo/releases/download/eoka-cli-v${version}"
scratch="$(mktemp -d)"
trap 'rm -rf "$scratch"' EXIT HUP INT TERM
fetch "$base/$asset" --output "$scratch/$asset"
fetch "$base/SHA256SUMS" --output "$scratch/SHA256SUMS"
expected="$(awk -v name="$asset" '$2 == name { print $1 }' "$scratch/SHA256SUMS")"
test "${#expected}" = 64 || { echo "SHA256SUMS has no entry for $asset" >&2; exit 1; }
test "$(sha256 "$scratch/$asset")" = "$expected" || { echo "Checksum mismatch for $asset" >&2; exit 1; }
tar -xzf "$scratch/$asset" -C "$scratch" eoka
test "$("$scratch/eoka" --version)" = "eoka $version" || { echo "Downloaded binary reports the wrong version" >&2; exit 1; }
mkdir -p "$install_dir"
install -m 755 "$scratch/eoka" "$install_dir/.eoka-new"
mv -f "$install_dir/.eoka-new" "$install_dir/eoka"
echo "Installed eoka $version to $install_dir/eoka"
case ":$PATH:" in *":$install_dir:"*) ;; *) echo "Add $install_dir to PATH to run it" ;; esac
