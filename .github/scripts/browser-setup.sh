#!/usr/bin/env bash
# Shared setup for the browser jobs: system packages, the Node SDK, pinned
# Chrome, and the staged native server/CLI built from this checkout.
set -euo pipefail

mkdir -p "$EOKA_SERVER_ARTIFACTS_DIR"
sudo apt-get update
sudo apt-get install -y xvfb xauth ffmpeg libnss3 libatk-bridge2.0-0 libgbm1 libasound2
mise exec -- npm --prefix clients/node ci --ignore-scripts --omit=optional
mise run node-build
mise exec -- node .github/scripts/install-chrome.mjs
# install-chrome.mjs exports through GITHUB_ENV, which this step can't see yet.
chrome_dir="$RUNNER_TEMP/eoka-rust-chrome"
sudo ln -sf "$chrome_dir/google-chrome" /usr/bin/google-chrome
/usr/bin/google-chrome --version
cargo build --locked -p eoka-server -p eoka-cli
cp target/debug/eoka-server "$EOKA_SERVER_ARTIFACTS_DIR/eoka-server-linux-amd64"
mise exec -- node clients/node/scripts/stage-release.mjs --artifacts "$EOKA_SERVER_ARTIFACTS_DIR" --output "$EOKA_RELEASE_STAGE_DIR" --host-only
# Ubuntu AppArmor blocks unprivileged user namespaces; Chrome's sandbox needs them.
sudo sysctl -w kernel.apparmor_restrict_unprivileged_userns=0
