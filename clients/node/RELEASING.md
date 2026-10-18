# Releasing the Node SDK

For maintainers. Users only need [README.md](README.md).

## Versions

| Piece | Version |
| --- | --- |
| npm SDK and five native packages | `0.1.0-beta.3` (tag `next`) |
| `eoka-server` | `0.6.1` |
| `eoka` core (crates.io) | `0.7.2` |
| Wire protocol | `1.3` |
| Chrome for Testing | `155.0.8059.39` |

All six npm packages share one version. When bumping it, update `package.json`, `release-manifest.json`, `scripts/publication-readiness.json`, the release scripts and tests, and `.github/workflows/publish-node.yml`. The core version and its registry checksum are pinned in `.github/workflows/qualify-node-release.yml`.

## Release flow

1. Merge the version bump to `main`. This publishes the crates and tags the server release.
2. Run **Publish Node prerelease** (`publish-node.yml`) from `main`, with `source_sha` set to the current `main` commit.
3. Approve the `npm-prerelease` deployment when it asks.

The workflow builds all five native targets, qualifies the packed tarballs on each platform, publishes the native packages and then the SDK under `next`, and finishes with a clean registry install and a browser smoke test.

If a run fails after some packages upload, re-run the failed jobs **in the same run**. Uploads that are already verified are kept. Never retag or republish a changed artifact. Roll forward with a new version instead.

## Publishing auth

Normal runs use npm trusted publishing (OIDC). Each of the six packages must have a trusted publisher configured for `shrimp-software/eoka-tools`, workflow `publish-node.yml`, environment `npm-prerelease`.

`bootstrap: true` is for a package's first publish only. It uses the environment secret `NPM_BOOTSTRAP_TOKEN`. Once trusted publishing is confirmed, delete that secret and revoke the token.

The `npm-prerelease` environment must require reviewers and allow only `main`. The workflow checks this before publishing. With several reviewers or a team, self-review must be blocked. A single user as the only reviewer (currently `cbxss`) may approve their own run.

## Local staging

Use this to build tarballs from server binaries without publishing:

```sh
npm ci --ignore-scripts --omit=optional
npm run build
npm run stage:release -- --artifacts /abs/server-artifacts --output /abs/stage --host-only
npm pack --ignore-scripts --pack-destination /abs/tarballs /abs/stage/platforms/linux-x64-gnu
npm pack --ignore-scripts --pack-destination /abs/tarballs /abs/stage/sdk
```

`--host-only` stages only the current platform and marks the stage as development. Without it, staging requires all five artifacts:

| Platform | Server artifact |
| --- | --- |
| linux-x64-gnu | `eoka-server-linux-amd64` |
| linux-arm64-gnu | `eoka-server-linux-arm64` |
| darwin-x64 | `eoka-server-darwin-amd64` |
| darwin-arm64 | `eoka-server-darwin-arm64` |
| win32-x64 | `eoka-server-windows-amd64.exe` |

To run the native consumer test against a stage:

```sh
EOKA_RELEASE_STAGE_DIR=/abs/stage EOKA_BROWSER_CACHE_DIR=/abs/cache npm run test:consumer
```

## Checks

```sh
npm run generate:check
npm run typecheck
npm test
npm run test:consumer
```

From the repo root, `mise run node-check` also checks the schema against the Rust exporter.

## Bumping Chrome

A new Chrome pin is a deliberate release. Update the build ID and archive SHA-256 pins in `release-manifest.json` and the build ID check in `src/release.ts`, then pass the full recording suite on every platform. Qualification records the real Chrome version, checks MP4 packet timestamps and decoder output, and fails on unknown diagnostics. Known release blockers are listed in `scripts/publication-readiness.json`.
