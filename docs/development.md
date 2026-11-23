# Development

Builds use the published eoka crate from crates.io at the version in the workspace
`Cargo.toml`. `Cargo.lock` pins its registry source and checksum; no sibling core
checkout or Cargo patch is required.

```sh
mise run drift
mise run release-check
cd clients/go && go test ./...
```

Without mise:

```sh
cargo test --workspace
bash scripts/release-check.sh
```

Use the local Tack override when testing unpublished Tack changes from a sibling
checkout:

```sh
bash scripts/release-check.sh --with-local-tack
```

Install the local CLI from this checkout:

```sh
mise run install-cli
```

Inspect the generated protocol-backed tool catalog:

```sh
eoka tools manifest --json
```

Run Tack TypeScript against the active browser session:

```sh
eoka tack 'return await tools.invoke({ path: "eoka.info", input: {} });'
```

## Builds without plugins

CI checks that each plugin crate still builds with its features off:

```sh
for package in eoka-solvers eoka-server eoka-mcp eoka-cli; do
  cargo clippy -p "$package" --all-targets --no-default-features -- -D warnings
done
```

Check one package at a time: a workspace build turns on every feature any member asks for and hides a broken slim build. The lock tests describe the default build and only run with every plugin on.

## Node SDK

```sh
npm --prefix clients/node ci --ignore-scripts --omit=optional
mise run node-generate
cargo build --locked -p eoka-server
mise run node-build
mise run sidecar-check
```

## Releasing

Every crate, the Node SDK and the Go client's server pin share one version, the
`[workspace.package]` version in `Cargo.toml`. To release:

```sh
node scripts/bump-release.mjs 0.19.0
```

Merge that to `main`. Publish puts every crate on crates.io, tags `v0.19.0`
and starts `release.yml`. That one run builds `eoka` and `eoka-mcp` for each
CLI target, qualifies the `eoka-server` binaries and publishes them all in the
GitHub release `v0.19.0` with one `SHA256SUMS`, then publishes npm. A merge
without a version bump publishes nothing. CI runs
`node scripts/bump-release.mjs --check` to catch files that drifted.

Dependabot releases itself. It only proposes versions published at least a
day ago. `dependabot-merge.yml` approves and auto-merges its minor and patch
updates once CI OK passes; major updates wait for review. `auto-release.yml`
then opens a `release: x.y.(z+1)` PR for updates outside `.github/`, waits
for its required checks and merges it with the owner's review bypass, after
any release in flight has finished. Both act
through `RELEASE_TOKEN`, a fine-grained token of the repository owner stored
as an Actions and a Dependabot secret, because merges made with
`GITHUB_TOKEN` start no workflows.

## Browser regression tests

Chrome-backed tests are `#[ignore]`d. Run the download suite with:

```sh
cargo test -p eoka-cli --test download_cli -- --ignored --test-threads=1
```

It also runs headed with `EOKA_DOWNLOAD_TEST_HEADED=1 xvfb-run -d`, and
optionally live with `EOKA_DOWNLOAD_TEST_LIVE=1`. Concurrent headed runs need
separate displays: use `xvfb-run -d` where supported, otherwise run the headed
suites one at a time.
