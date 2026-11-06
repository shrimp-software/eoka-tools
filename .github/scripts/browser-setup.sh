#!/usr/bin/env bash
# Per-job setup for the browser suites, after the shared browser-build
# artifact is unpacked: system packages, SDK dependencies, the pinned Chrome,
# and the staged native client.
set -euo pipefail

mkdir -p "$EOKA_SERVER_ARTIFACTS_DIR"
apt=(sudo apt-get -o Acquire::Retries=3 -o Acquire::http::Timeout=30)
"${apt[@]}" update
"${apt[@]}" install -y xvfb xauth ffmpeg libnss3 libatk-bridge2.0-0 libgbm1 libasound2
mise exec -- npm --prefix clients/node ci --ignore-scripts --omit=optional
mise exec -- node .github/scripts/install-chrome.mjs
# install-chrome.mjs exports through GITHUB_ENV, which this step can't see yet.
sudo ln -sf "$RUNNER_TEMP/eoka-rust-chrome/google-chrome" /usr/bin/google-chrome
/usr/bin/google-chrome --version
cp target/debug/eoka-server "$EOKA_SERVER_ARTIFACTS_DIR/eoka-server-linux-amd64"
mise exec -- node clients/node/scripts/stage-release.mjs --artifacts "$EOKA_SERVER_ARTIFACTS_DIR" --output "$EOKA_RELEASE_STAGE_DIR" --host-only
# Ubuntu AppArmor blocks unprivileged user namespaces; Chrome's sandbox needs them.
sudo sysctl -w kernel.apparmor_restrict_unprivileged_userns=0
