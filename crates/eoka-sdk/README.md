# eoka-sdk

Rust client for named eoka sessions.

The SDK talks to a session's daemon over its Unix socket using the eoka wire
protocol (see [docs/protocol.md](../../docs/protocol.md#session-socket)), and
starts the daemon when a command needs one. It is intended for Rust
integrations that need browser automation without shelling out to the CLI for
every command.

The daemon itself is the `eoka` CLI. The SDK starts it from `EOKA_BIN` when
set, otherwise from `eoka` on `PATH`, so install the CLI alongside your
program.
