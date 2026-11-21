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

Each Node release bundles the server version recorded in
`clients/node/release-manifest.json`. Merging a version bump to `main`
publishes every crate whose version is not yet on crates.io and starts the npm
release.

## Browser regression tests

Chrome-backed tests are `#[ignore]`d. Run the download suite with:

```sh
cargo test -p eoka-cli --test download_cli -- --ignored --test-threads=1
```

It also runs headed with `EOKA_DOWNLOAD_TEST_HEADED=1 xvfb-run -d`, and
optionally live with `EOKA_DOWNLOAD_TEST_LIVE=1`. Concurrent headed runs need
separate displays: use `xvfb-run -d` where supported, otherwise run the headed
suites one at a time.
