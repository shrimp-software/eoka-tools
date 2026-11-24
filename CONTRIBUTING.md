# Contributing

Thanks for helping improve eoka-tools.

## Development

Install a recent stable Rust toolchain. The Go SDK also requires Go 1.21 or
newer.

```bash
cargo fmt --all --check
cargo clippy --all-features -- -D warnings
cargo test --all-features
```

For the Go SDK:

```bash
cd clients/go
test -z "$(gofmt -l .)"
go vet ./...
go test ./... -race
```

## Public surface locks

Three files pin what clients see: the MCP tool list
(`crates/eoka-mcp/tests/locks/tools.jsonl`), what engine-backed MCP tools reply
(`crates/eoka-mcp/tests/locks/replies.jsonl`, checked by an ignored test that
needs Chrome) and the CLI tool manifest
(`crates/eoka-cli/tests/locks/manifest.jsonl`). A test fails when one of them
changes. If the change is intended, regenerate the file and list the change in
the pull request:

```bash
EOKA_UPDATE_LOCKS=1 cargo test -p eoka-mcp the_tool_list_a_client_sees_is_locked
EOKA_UPDATE_LOCKS=1 cargo test -p eoka-mcp every_mcp_tool_answers -- --ignored
EOKA_UPDATE_LOCKS=1 cargo test -p eoka-cli the_tool_manifest_agents_read_is_locked
```

The environment variable table in `docs/cli.md` is generated from
`crates/eoka-protocol/src/env.rs`. Register a new `EOKA_*` variable there, read
it through its constant, and regenerate the table:

```bash
EOKA_UPDATE_LOCKS=1 cargo test -p eoka-protocol env::
```

## Pull requests

Keep changes focused and update the relevant crate README when user-facing
behavior changes. Workspace crates should keep their `Cargo.toml` metadata ready
for crates.io.
