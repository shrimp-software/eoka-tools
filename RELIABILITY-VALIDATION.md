# Login and navigation implementation validation

Local qualification on 2026-10-31. The changes are in the paired
`fix/login-navigation-reliability` branches, based on core `659fe31` and tools
`cacc76d`. The original checkouts and investigation transcript were preserved.
No live account credentials were used in fixtures.
The reviewed core implementation is committed as `d7f55bd`.

## Environment and dependency boundary

Linux x86_64; Rust 1.94.1 with a Rust 1.85.0 library check; Go 1.21.5;
Node 24.21.0. Rust browser tests and direct Node/Go fixtures used Chrome
157.0.8103.15 beta. The installed Node package test used the checksum-verified
Chrome for Testing 155.0.8059.39 cache and a locally staged native server.
`TMPDIR=/var/tmp/er` kept Chrome sockets on disk with short paths.

Tools were built against the paired core checkout through an external Cargo
configuration. This is development validation, not registry/release evidence.
The equivalent explicit override is:

```sh
cargo --config 'patch.crates-io.eoka.path="../eoka-reliability"' test --workspace --all-features
```

The complete tools browser gate also supplies the explicit executable required
by the runtime/viewport fixture:

```sh
TMPDIR=/var/tmp/er EOKA_TEST_CHROME_PATH=/opt/google/chrome-beta/chrome \
  xvfb-run -a cargo --config 'patch.crates-io.eoka.path="../eoka-reliability"' \
  test --workspace --all-features --locked --no-fail-fast -- --ignored --test-threads=1
```

## Checks

| Gate | Local result |
|---|---|
| Core formatting, all-target/all-feature Clippy | Passed |
| Core all-feature unit/integration/doc tests without Chrome | 235 passed |
| Core complete ignored Chrome suite, serial under Xvfb | 92 passed |
| Core examples and Rust 1.85.0 library check | Passed |
| Tools formatting, workspace check, all-target/all-feature Clippy | Passed |
| Tools workspace unit/doc tests | 362 passed |
| Tools complete ignored Chrome suite, serial under Xvfb | 102 passed |
| Protocol projection conformance | Passed |
| Surface parity | 64 sidecar methods checked |
| Node generation, types, runtime, package and real-browser tests | 322 passed; zero skipped |
| Go formatting, vet, race suite and real-browser fixture | Passed; uncached race run |
| Sketchy helper fixtures and real local HLS download | 6 passed |
| CLI installer | Original PR validation: exact compiled binary installed; bad checksum and wrong version rejected without replacing the existing binary. Installer unchanged and not rerun for this review revision |
| New/changed CI workflows | Hosted runners only; browser workflow actionlint check retained from original PR. Revised publisher passes actionlint, shell syntax and CLI/server release-dispatch guard; registry qualification remains blocked on core 0.8.0 |
| `mise run release-check` | Quality gates passed; registry gate correctly rejected the local core override |

The new hosted browser CI job runs all Rust browser tests, all Node tests with
native host artifacts, Go race/browser tests, and helper tests. Host artifact
staging is explicitly development-only.

## Behavior exercised

- A headed session is reused by its literal name; an explicit conflicting mode
  is rejected while the existing page marker remains intact.
- Ordinary reads preserve scoped control references. Pagination exhausts a
  251-control document; filtering preserves references; replacing a node makes
  its original reference unusable.
- Sequential Unicode input produces trusted native input events; explicit bulk
  input produces one event. Native Tab triggers blur. Readonly and disabled
  targets retain useful errors. A partial action is never replayed.
- Moving focus after the first character stops further input. The regression
  failed before the repair by placing the remaining text in the other field.
- Operation status and cancellation remain responsive during both ordinary fill
  and agent-step input. Cancellation stops partial input without replaying it;
  the operation remains inspectable independently of the serial command handler.
- Popup verification follows the originating tab's child, waits for URL and
  enabled-state conditions, and returns the destination ID even when the
  script-opened origin closes during verification.
- Target destruction interrupts an unanswered verification read, preserving the
  remaining deadline for surviving popup candidates. A deterministic CDP peer
  reproduces the stalled read before the fix and passes afterward. All nine core
  reliability fixtures also pass on pinned Chrome for Testing 155.0.8059.39.
- CLI and MCP recover the original operation after automatically selecting its
  popup. Identical retries do not click again; changed expectations and explicit
  page changes still produce operation-ID conflicts.
- Strict actions reject missing expectations; deliberate exploration reports
  uncertainty. Operation IDs and failure details survive recovery. Ambiguous
  admission/storage failures retain an unknown outcome rather than claiming
  that nothing was dispatched.
- Scoped function calls preserve JSON keys and Unicode arguments, fresh locals,
  exceptions, promise rejection and result limits. Core owns the shared JSON
  validation before CDP serialization, rejecting lossy/non-JSON values and
  enforcing the one-MiB UTF-8 boundary consistently across scoped clients.
  Validation failure invokes the function only once; execution discards cyclic
  return values while preserving exceptions. Raw unawaited execution is
  explicitly uncertain.
- Recorder start/stop/clear acknowledge state changes. Immediate navigation is
  captured and navigation still works after recording stops.
- Helpers select the requested lesson rather than the previously open lesson,
  reject login redirects/missing metadata, preserve uncertain exits, validate a
  real local HLS download, refuse overwrite and clean only their own scratch.

## Outstanding release gates

1. Run the authenticated macOS workflow with the revised CLI and all three
   helpers: login/blur/validation, navigation, lesson selection, listing and a
   verified download. This Linux environment has not established that result.
2. Publish core 0.8.0 first, regenerate the tools lockfile from crates.io, and
   pass the registry-only release check. A current registry query found no
   `eoka@0.8.0`; the checked-in tools lockfile still represents local validation.
3. Qualify version-matched CLI/server binaries on each supported platform and
   complete Node publication qualification. No packages, tags or binaries have
   been published by this work. Node publication readiness remains blocked in
   `clients/node/scripts/publication-readiness.json`.

Review revision logs are in `/home/cbass/.cache/er/`: `fix-core-unit-final.log`,
`fix-core-browser-final.log`, `fix-core-clippy-final.log`,
`fix-core-msrv-final.log`, `fix-core-examples-final.log`,
`fix-core-reliability-pinned.log`, `fix-core-lifecycle-negative.log`,
`fix-tools-release-check-final.log`, `fix-tools-browser-lifecycle.log`,
`fix-tools-conformance-final.log`, `fix-surface-parity.log`,
`fix-node-check-final.log`, `fix-go-vet.log`, `fix-go-tests-final.log` and
`fix-helper-tests.log`. Original installer evidence remains in
`installer-check.log`.
