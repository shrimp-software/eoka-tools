# eoka-protocol

Protocol types and tool catalog metadata for Eoka browser automation.

This crate is the source of truth for daemon request wire commands, stable tool
paths, operation metadata, input schemas, and manifest projection. Downstream
surfaces such as the CLI manifest, Tack adapter, SDK, and MCP integration use
these definitions to avoid drift.

The `sidecar` module defines the NDJSON envelopes, lifecycle negotiation,
structured errors and the actual implemented sidecar method registry. It is
separate from the daemon/CLI catalog. Export complete per-method input and result
JSON Schemas with `cargo run -p eoka-protocol --bin export-sidecar-schema`.

Sidecar request IDs are positive integers through `9007199254740991`. Integer
fields in exported schemas retain their Rust bounds and are capped at the
JavaScript exact integer range; runtime validation rejects unsafe integers rather
than rounding. Responses contain exactly one `result` or `error`, including
successful `null` results. Action and CDP errors retain structured evidence.

Evaluation preserves the existing `{ "result": ... }` wire wrapper and accepts
only JSON-compatible data. Explicit `null` is successful; undefined, functions,
symbols, BigInt, non-finite or unsafe integral numbers, array holes, cycles and
non-JSON objects are errors. `page.execute` discards its return value.

Protocol 1.3 preserves the screenshot `{dataBase64}` result while adding typed
PNG/JPEG/WebP options and direct non-overwrite file output. In-memory screenshot
bytes are bounded at 16 MiB. Artifact metadata reports absolute paths and
JavaScript-safe byte counts; native video is MP4 with Chrome-selected codec.

Canonical recording start/get/stop/cancel DTOs use caller-known UUIDs and expose
running/finalizing/completed/cancelled/failed states. Completed records require a
published artifact; failures preserve stage, code, retained private partial path
and uncertain-cleanup evidence. Recovery is live-session-scoped, never automatic
restart. Method/capability negotiation does not guarantee a browser encoder.
The Node contracts and bundled schemas are generated from this module. See
[the wire protocol](../../PROTOCOL.md#file-artifacts-and-native-recording) for
transition, retry, cancellation and file-publication semantics.

