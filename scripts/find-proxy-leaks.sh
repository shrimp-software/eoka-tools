#!/usr/bin/env bash
# find-proxy-leaks.sh — Hunt for real leaked proxy credentials on GitHub
#
# Targets config files, CI/CD, test fixtures, and env files with actual
# credential URLs — not documentation, tutorials, or placeholder examples.
#
# Usage:
#   ./find-proxy-leaks.sh [--limit N] [--check-live] [--mode quick|deep]
#   --limit N      Max search results per query (default: 50)
#   --check-live   Test each candidate to see if the proxy still responds
#   --mode quick   (default) only high-signal queries
#   --mode deep    also search broader patterns
#
# Requires: gh (authenticated), jq, curl, rg

set -euo pipefail
umask 077

LIMIT=50
CHECK_LIVE=false
MODE=quick

while [[ $# -gt 0 ]]; do
  case "$1" in
    --limit) LIMIT="$2"; shift 2 ;;
    --check-live) CHECK_LIVE=true; shift ;;
    --mode) MODE="$2"; shift 2 ;;
    *) echo "Unknown: $1"; exit 1 ;;
  esac
done

for cmd in gh jq curl rg; do
  command -v "$cmd" >/dev/null 2>&1 || { echo "Missing: $cmd"; exit 1; }
done
gh api user >/dev/null 2>&1 || { echo "gh not authenticated"; exit 1; }

# Regex for proxy credentials: http[s]://user:pass@host:port
CRED_RE="https?://[^@[:space:]\"><]+:[^@[:space:]\"><]+@[0-9a-zA-Z._-]+:[0-9]+"

# Exclude documentation/example repos and placeholder patterns.
# Real leaks don't use these.
EXCLUDE_RE="(example\.com|localhost|127\.0\.0\.1|0\.0\.0\.0|\.invalid|test\.com|your-proxy|proxy\.example|PRIVATE|placeholder|changeme|sample|REPLACE|your_password|enter-password|YOUR_PROXY|ACCOUNT:|password%21|companyId999|admin:admin|user:pass|user2:pass2|droberson/rtfm|HariSekhon/Knowledge-Base|Dataman-Cloud/kelisa)"

echo "===== Proxy Credential Leak Scanner ====="
echo "Mode: $MODE  Limit/query: $LIMIT  Check live: $CHECK_LIVE"
echo ""

# ─── Query construction ───
# We use GitHub's path qualifier to target specific file types where real
# credentials live: configs, CI/CD, tests, env files — NOT documentation.
#
# File types to search (high signal):
#   path:.env  path:.yml  path:.yaml  path:.json  path:.toml
#   path:.ini  path:.cfg  path:.conf  path:.properties
#   path:Dockerfile  path:docker-compose
#   path:.gitlab-ci.yml  path:.github/workflows
#   path:test*  path:*spec*  path:*config*

QUERIES=()

# ── Tier 1: High signal in config files ──
# These are almost never documentation — they're real config.
QUERIES+=('AUTH_PROXY http:// path:.env')
QUERIES+=('AUTH_PROXY http:// path:.yml')
QUERIES+=('AUTH_PROXY http:// path:.yaml')
QUERIES+=('AUTH_PROXY http:// path:.json')
QUERIES+=('AUTH_PROXY http:// path:.toml')

QUERIES+=('PROXY_URL http:// path:.env')
QUERIES+=('PROXY_URL http:// path:.yml')
QUERIES+=('PROXY_URL http:// path:.yaml')
QUERIES+=('PROXY_URL http:// path:.json')
QUERIES+=('PROXY_URL http:// path:.toml')

QUERIES+=('http_proxy http:// path:.env')
QUERIES+=('http_proxy http:// path:.yml')
QUERIES+=('http_proxy http:// path:.yaml')
QUERIES+=('http_proxy http:// path:.json')
QUERIES+=('http_proxy http:// path:.toml')

QUERIES+=('HTTPS_PROXY http:// path:.env')
QUERIES+=('HTTPS_PROXY http:// path:.yml')
QUERIES+=('proxy_password http:// path:.env')
QUERIES+=('proxy_password http:// path:.yml')
QUERIES+=('proxy_pass http:// path:.env')

# ── Tier 2: CI/CD paths ──
QUERIES+=('http_proxy http:// path:.github/workflows')
QUERIES+=('HTTPS_PROXY http:// path:.github/workflows')
QUERIES+=('http_proxy http:// path:.gitlab-ci.yml')
QUERIES+=('proxy http:// path:.github/workflows')

# ── Tier 3: Test files (real creds in test fixtures) ──
QUERIES+=('AUTH_PROXY path:*test* http://')
QUERIES+=('PROXY_URL path:*test* http://')
QUERIES+=('http_proxy path:*test* http://')
QUERIES+=('proxy host path:*test* http://')

# ── Tier 4: Docker ──
QUERIES+=('http_proxy http:// path:Dockerfile')
QUERIES+=('ENV http_proxy http:// path:Dockerfile')
QUERIES+=('ENV https_proxy http:// path:Dockerfile')

if [[ "$MODE" == "deep" ]]; then
  # Broader searches — more noise but might catch unusual leak locations
  QUERIES+=('http_proxy http:// path:.properties')
  QUERIES+=('HTTPS_PROXY http:// path:.properties')
  QUERIES+=('proxy.http:// path:.yml')
  QUERIES+=('proxy.http:// path:.yaml')
  QUERIES+=('"proxy" "http://" path:settings')
fi

declare -A seen
TOTAL=0

for q_idx in "${!QUERIES[@]}"; do
  q="${QUERIES[$q_idx]}"
  echo -n "  [$((q_idx+1))/${#QUERIES[@]}] $q"

  raw=$(gh search code --limit "$LIMIT" --json repository,path,url -- "$q" 2>/dev/null || true)
  if [[ -z "$raw" || "$raw" == "[]" || "$raw" == "null" ]]; then
    echo " → 0"
    continue
  fi

  count=$(echo "$raw" | jq '. | length' 2>/dev/null || echo "0")
  echo " → $count"

  for row in $(echo "$raw" | jq -r '.[] | @base64' 2>/dev/null); do
    item=$(echo "$row" | base64 -d 2>/dev/null || echo "")
    [[ -z "$item" ]] && continue

    repo=$(echo "$item" | jq -r '.repository.nameWithOwner // ""')
    filepath=$(echo "$item" | jq -r '.path // ""')
    link=$(echo "$item" | jq -r '.url // ""')
    [[ -z "$repo" || -z "$filepath" ]] && continue

    # Skip docs and binaries
    case "$filepath" in
      *.md|*.rst|*.txt|*.png|*.jpg|*.jpeg|*.gif|*.svg|*.ico|*.woff*|*.ttf|*.eot|*.o|*.so|*.dll|*.exe|*.jar|*.zip|*.gz|*.tar|*.tgz|*.bz2) continue ;;
    esac

    # Fetch raw file
    content=$(curl -sL --max-time 8 \
      "https://raw.githubusercontent.com/$repo/HEAD/$filepath" 2>/dev/null || true)
    [[ -z "$content" ]] && continue

    # Scan for credential URLs
    creds=$(echo "$content" | rg -o "$CRED_RE" 2>/dev/null || true)
    [[ -z "$creds" ]] && continue

    while IFS= read -r cred; do
      [[ -z "$cred" ]] && continue
      # Skip false positives (docs, examples, placeholders)
      if echo "$cred" | rg -qi "$EXCLUDE_RE" 2>/dev/null; then continue; fi

      # Dedup
      if [[ -n "${seen[$cred]:-}" ]]; then continue; fi
      seen[$cred]=1
      TOTAL=$((TOTAL + 1))

      # Extract fields for display
      user=$(echo "$cred" | sed 's|http://||;s|https://||' | sed 's|:.*||')
      host=$(echo "$cred" | sed 's|.*@||' | sed 's|:[0-9]*$||')
      port=$(echo "$cred" | sed 's|.*:||')

      echo ""
      echo "████████ #${TOTAL} ████████████████████████████████████"
      echo "  URL:    $cred"
      echo "  User:   $user"
      echo "  Host:   $host:$port"
      echo "  Repo:   $repo"
      echo "  File:   $filepath"
      echo "  Link:   $link"

      if $CHECK_LIVE; then
        echo -n "  Live:   "
        code=$(timeout 6 curl -s -o /dev/null -w "%{http_code}" \
          -x "$cred" http://httpbin.org/ip 2>/dev/null || echo "unreachable")
        case "$code" in
          200) echo "⚠️  YES (200)" ;;
          301|302|307|308) echo "⚠️  YES (${code})" ;;
          407) echo "⚠️  YES (auth required)" ;;
          000|unreachable) echo "❌ NO" ;;
          *) echo "⚡ HTTP $code" ;;
        esac
      fi
    done < <(echo "$creds")
  done
done

echo ""
echo "══════════════════════════════════════════════"
echo "  DONE — $TOTAL unique proxy credential(s) found"
echo "══════════════════════════════════════════════"
echo ""
if $CHECK_LIVE && [[ "$TOTAL" -gt 0 ]]; then
  echo "⚠️  Any proxy marked LIVE is currently usable by anyone who has the URL."
  echo "   Responsible disclosure: open a GitHub issue describing the leak"
  echo "   (without including the credential in the issue body)."
fi