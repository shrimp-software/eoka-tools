#!/usr/bin/env bash
# Shared libraries a downloaded Chrome needs on a bare Ubuntu runner.
set -euo pipefail
apt=(sudo apt-get -o Acquire::Retries=3 -o Acquire::http::Timeout=30)
"${apt[@]}" update
"${apt[@]}" install -y --no-install-recommends libasound2 libatk1.0-0 libatk-bridge2.0-0 libcups2 libdrm2 libgbm1 libgtk-3-0 libnss3 libx11-xcb1 libxcomposite1 libxdamage1 libxfixes3 libxrandr2 fonts-liberation
