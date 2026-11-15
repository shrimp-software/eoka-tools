# eoka-sdk

Rust SDK for controlling Eoka browser automation sessions.

The SDK wraps Eoka daemon launch and protocol request/response handling behind
typed session helpers. It is intended for Rust integrations that need direct
browser automation without shelling out to the CLI for every command.

The daemon itself is the `eoka` CLI. The SDK starts it from `EOKA_BIN` when
set, otherwise from `eoka` on `PATH`, so install the CLI alongside your
program.

