#!/bin/sh
# Install the eoka CLI and the eoka-mcp server from a GitHub release.
#
#   EOKA_VERSION      release to install (default: the latest release)
#   EOKA_INSTALL_DIR  destination directory (default: ~/.local/bin)
#   GITHUB_TOKEN      optional, raises the GitHub API rate limit
#
# An installed CLI updates both binaries with `eoka update`. Installed copies
# of the agent skill are rewritten to match the binary this script installs.
set -eu
repo="shrimp-software/eoka-tools"
fetch() { curl --fail --location --silent --show-error "$@"; }

version="${EOKA_VERSION:-}"
if [ -z "$version" ]; then
    # The API answers with either indented or single-line JSON, so put every
    # field on its own line before reading the tag.
    version="$(fetch ${GITHUB_TOKEN:+--header "Authorization: Bearer $GITHUB_TOKEN"} \
        "https://api.github.com/repos/$repo/releases/latest" |
        tr ',' '\n' |
        sed -n 's/.*"tag_name": *"v\([0-9][0-9.]*\)".*/\1/p' |
        head -n 1)"
    test -n "$version" || { echo "Could not find the latest eoka release; set EOKA_VERSION" >&2; exit 1; }
fi
case "$version" in *[!0-9.]*|'') echo "Invalid EOKA_VERSION" >&2; exit 2;; esac

case "$(uname -s)-$(uname -m)" in
    Darwin-arm64) target=aarch64-apple-darwin ;;
    Darwin-x86_64) target=x86_64-apple-darwin ;;
    Linux-aarch64|Linux-arm64) target=aarch64-unknown-linux-gnu ;;
    Linux-x86_64) target=x86_64-unknown-linux-gnu ;;
    *) echo "Unsupported platform; use cargo install eoka-cli eoka-mcp" >&2; exit 1 ;;
esac
if command -v sha256sum >/dev/null 2>&1; then
    sha256() { sha256sum "$1" | awk '{print $1}'; }
elif command -v shasum >/dev/null 2>&1; then
    sha256() { shasum -a 256 "$1" | awk '{print $1}'; }
else
    echo "Need sha256sum or shasum to verify the download" >&2; exit 1
fi

asset="eoka-${target}.tar.gz"
install_dir="${EOKA_INSTALL_DIR:-$HOME/.local/bin}"
base="https://github.com/$repo/releases/download/v${version}"
scratch="$(mktemp -d)"
trap 'rm -rf "$scratch"' EXIT HUP INT TERM
fetch "$base/$asset" --output "$scratch/$asset"
fetch "$base/SHA256SUMS" --output "$scratch/SHA256SUMS"
expected="$(awk -v name="$asset" '$2 == name { print $1 }' "$scratch/SHA256SUMS")"
test "${#expected}" = 64 || { echo "SHA256SUMS has no entry for $asset" >&2; exit 1; }
test "$(sha256 "$scratch/$asset")" = "$expected" || { echo "Checksum mismatch for $asset" >&2; exit 1; }
tar -xzf "$scratch/$asset" -C "$scratch" eoka eoka-mcp
for binary in eoka eoka-mcp; do
    reported="$("$scratch/$binary" --version 2>/dev/null)" || { echo "The downloaded $binary cannot run here. On Linux it needs glibc 2.35 or newer. Install from source instead: cargo install eoka-cli eoka-mcp" >&2; exit 1; }
    test "$reported" = "$binary $version" || { echo "Downloaded $binary reports the wrong version" >&2; exit 1; }
done
mkdir -p "$install_dir"
for binary in eoka-mcp eoka; do
    install -m 755 "$scratch/$binary" "$install_dir/.$binary-new"
    mv -f "$install_dir/.$binary-new" "$install_dir/$binary"
done
echo "Installed eoka and eoka-mcp $version to $install_dir"
"$install_dir/eoka" skill refresh 2>/dev/null || true
case ":$PATH:" in *":$install_dir:"*) ;; *) echo "Add $install_dir to PATH to run them" ;; esac
