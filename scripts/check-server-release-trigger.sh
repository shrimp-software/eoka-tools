#!/usr/bin/env bash
set -euo pipefail

publish_workflow=.github/workflows/publish.yml
release_workflow=.github/workflows/release-eoka-server.yml

grep -Fqx '      actions: write' "$publish_workflow"
grep -Fqx '  workflow_dispatch:' "$release_workflow"
grep -Fqx '      - "eoka-server-v*"' "$release_workflow"

awk '
  /git push origin "\$tag"/ { tag_pushed = 1 }
  tag_pushed && /if \[ "\$crate" = "eoka-server" \]; then/ { server_dispatch = 1 }
  server_dispatch && /GH_TOKEN="\$\{\{ github.token \}\}" gh workflow run release-eoka-server.yml --repo "\$\{\{ github.repository \}\}" --ref "\$tag" -f tag="\$tag"/ { valid = 1; exit }
  END { exit !valid }
' "$publish_workflow"
