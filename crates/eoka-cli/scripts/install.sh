#!/bin/sh
set -eu
version="${EOKA_VERSION:-0.7.0}"
case "$version" in *[!0-9.]*|'') echo "Invalid EOKA_VERSION" >&2; exit 2;; esac
case "$(uname -s)-$(uname -m)" in
    Darwin-arm64) target=aarch64-apple-darwin ;;
    Darwin-x86_64) target=x86_64-apple-darwin ;;
    Linux-aarch64) target=aarch64-unknown-linux-gnu ;;
    Linux-x86_64) target=x86_64-unknown-linux-gnu ;;
    *) echo "Unsupported platform; use cargo install eoka-cli" >&2; exit 1 ;;
esac
asset="eoka-cli-${target}.tar.gz"
install_dir="${EOKA_INSTALL_DIR:-$HOME/.local/bin}"
base="https://github.com/shrimp-software/eoka-tools/releases/download/eoka-cli-v${version}"
scratch="$(mktemp -d)"
trap 'rm -rf "$scratch"' EXIT HUP INT TERM
curl --fail --location --silent --show-error "$base/$asset" --output "$scratch/$asset"
curl --fail --location --silent --show-error "$base/SHA256SUMS" --output "$scratch/SHA256SUMS"
expected="$(awk -v name="$asset" '$2 == name { print $1 }' "$scratch/SHA256SUMS")"
test "${#expected}" = 64
actual="$(shasum -a 256 "$scratch/$asset" | awk '{print $1}')"
test "$actual" = "$expected"
tar -xzf "$scratch/$asset" -C "$scratch" eoka
test "$("$scratch/eoka" --version)" = "eoka $version"
mkdir -p "$install_dir"
install -m 755 "$scratch/eoka" "$install_dir/.eoka-new"
mv -f "$install_dir/.eoka-new" "$install_dir/eoka"
echo "Installed eoka $version to $install_dir/eoka"
