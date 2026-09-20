#!/usr/bin/env bash
set -euo pipefail

publish_workflow=.github/workflows/publish.yml
release_workflow=.github/workflows/release-eoka-server.yml

grep -Fqx '      actions: write' "$publish_workflow"
grep -Fqx '  workflow_dispatch:' "$release_workflow"
grep -Fqx '      - "eoka-server-v*"' "$release_workflow"
grep -Fqx '          for crate in captcha eoka-proxy eoka-protocol eoka-sdk eoka-email eoka-runner eoka-server eoka-mcp eoka-tack datadome eoka-cli; do' "$publish_workflow"

awk '
  /if \[ "\$http_status" = "200" \]; then/ { published_status = 1 }
  published_status && /continue/ { continued = 1 }
  published_status && /already on crates.io — reconcile release/ { published_reconciliation = 1 }
  /if git rev-parse "\$tag" >\/dev\/null 2>&1; then/ { tag_reconciliation = 1 }
  tag_reconciliation && /Tag \$\{tag\} already exists — reconcile release/ { existing_tag_reconciliation = 1 }
  /if \[ "\$crate" = "eoka-server" \]; then/ { server_reconciliation = 1 }
  server_reconciliation && /GH_TOKEN="\$\{\{ github.token \}\}" gh release view "\$tag" --repo "\$\{\{ github.repository \}\}" >\/dev\/null 2>&1/ { release_check = 1 }
  release_check && /GH_TOKEN="\$\{\{ github.token \}\}" gh workflow run release-eoka-server.yml --repo "\$\{\{ github.repository \}\}" --ref "\$tag" -f tag="\$tag"/ { dispatch = 1 }
  END { exit !(published_reconciliation && !continued && existing_tag_reconciliation && release_check && dispatch) }
' "$publish_workflow"
