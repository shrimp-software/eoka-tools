# eoka-datadome (experimental)

A local slider solver. **No AI/ML model, Python/OpenCV runtime or paid solving
service is required.** Simple sliders use visible geometry and native mouse
input. Image puzzles use alpha-aware edge matching and normalized correlation.

One real simple-slider-to-password transition and one later authenticated
SoundCloud login were observed. The authenticated run encountered no new CAPTCHA;
these are not proof of universal acceptance or a fresh CAPTCHA-to-login chain.
Image-puzzle matching remains synthetic-test-only.

## CLI: continue in the existing session

Install the updated CLI with `mise run install-cli`. This crate requires published
Eoka 0.5.13 or later; no sibling checkout is needed. Older daemons must be upgraded
explicitly.

```sh
eoka --session demo open https://example.com/protected
eoka --session demo captcha datadome
eoka --session demo snapshot
```

If already on the challenged page, run only the last two commands. Preserve the
same `--session` and, when applicable, `--cdp` connection flags. The command
requires an existing daemon/browser/tab and brings the selected tab to the
foreground. It never creates another browser, imports cookies, closes the tab,
changes proxy/profile configuration or replays the action that hit the challenge.
It does not enter credentials or fall back to a paid provider.

```sh
eoka --session demo --json captcha datadome --max-attempts 2 --timeout-ms 60000
```

Defaults: one attempt, 60000 ms. Attempts must be 1–4; the solve budget must be
5000–120000 ms, with up to five seconds of additional bounded cleanup. The budget
includes foregrounding and detection. Image matching runs off the async runtime
with a single-worker limit and cooperative cancellation. Timed-out drags release
held input; unsuccessful cleanup is reported, not disguised as a solve.

The standard response envelope contains `data.outcome`, `data.tab_id` and
`data.elapsed_ms` for solver outcomes. `challenge_cleared` and `not_present` exit
zero; the latter explicitly means no recognized challenge, not successful access.
`blocked`, `ip_banned` and `failed` retain outcome data but exit nonzero. Other
errors have stable `error_detail.code` values, including `session_unavailable`,
`no_active_tab`, `unsupported_operation`, `datadome_timeout`,
`datadome_cleanup_failed` and `datadome_transport_error`. Cookie values, page
bodies and credentials are not returned by this command.

Request interception and network recording remain active. Owned request pauses
are serviced while solving; interception loss/failure prevents a clearance claim.
Existing user-configured diagnostic/capture settings are not silently disabled.
Interception response/capture files must be regular opened descriptors and are
capped at 8 MiB, with a 500 ms file deadline and one file worker. Special files,
including FIFOs/devices, are rejected without blocking open. A filesystem syscall
already in progress cannot be cancelled; its worker retains the single permit
until exit, bounding residual work without blocking daemon shutdown. Request
operations and never-dispatched fallback confirmations each have a one-second
deadline. A possibly dispatched fulfillment/continuation is never replayed after
cancellation, timeout or error: missing acknowledgment remains unconfirmed.
Shutdown retains queued/in-flight ownership and hands off only never-dispatched
pauses. The command snapshots integrity at entry and transfers the idle worker
into its solve budget, so transition failures cannot become a clean baseline.
A request already accepted by the daemon continues within its budget if the CLI
disconnects. A transport error therefore means an uncertain result: inspect the
existing page before manually retrying. Neither transport failures nor an old
daemon trigger automatic restart or replay.

The SDK exposes `EokaClient::captcha_datadome(CaptchaDatadomeArgs)`. The canonical
operation is `captcha.datadome` (`captcha_datadome` on the daemon), opt-in under
the CAPTCHA capability; CLI manifests and Tack project that same definition.
The separate native MCP and JSON-RPC server execution paths are not wired to this
solver. Existing paid `captcha solve` and token injection commands are unchanged.

## Library

```rust
use eoka_datadome::{DataDomeSolver, SolveOutcome};

let outcome = DataDomeSolver::new().solve(&page).await?;
println!("Challenge cleared: {}", outcome == SolveOutcome::ChallengeCleared);
```

`ChallengeCleared` means a changed nonempty cookie and sustained absence of the
challenge, not authenticated access. The other outcomes are `NotPresent`,
`Blocked`, `IpBanned` and `Failed`. A two-second completion settle prevents the
reproduced delayed re-block false positive. Completion inspects the selected main
document and surviving challenge/ancestor lineage for block notices, even at
non-challenge URLs. Inspection errors cannot establish clearance.

- Handles nested/OOPIF frames and positive axis-aligned scaling/translation.
- Simple sliders use a constrained horizontal drag without overshoot.
- Readable image pairs use local matching, capped at 1024×512 pixels. Extraction
  rejects more than 64 image elements, eight candidates or 1,048,576 aggregate
  pixels before canvas conversion. PNG data URLs are capped at 3 MiB each/6 MiB
  total; Rust PNG decoding also enforces dimensions and an 8 MiB allocation limit.
- Image content must fill an unbordered/unpadded box, with positive axis-aligned
  transforms throughout its ancestry; reflection, rotation/skew and unsupported
  object placement fail before any drag. Observable slot/host ancestry is checked
  with a 64-step bound. Selected images also require positive axis-aligned ordered
  native content quads; this catches closed-shadow slot reflection that DOM
  `assignedSlot` hides. Geometry is snapshot-time, not a guarantee against later
  page mutation; stale, missing or uncertain native geometry fails closed.
- Rejects weak matching, ambiguous geometry and unsupported transforms.
- Canvas-only and CORS-tainted image puzzles remain unsupported.
- `solve_with_drag(page, callback)` optionally supplies custom input delivery.
  Its `SliderDrag` coordinates are root-viewport CSS pixels; the callback returns
  `eoka::Result<()>`. Completion checks remain mandatory.

## Bounded example

```sh
mise exec -- cargo run -p eoka-datadome --example datadome_solve -- TARGET_URL
```

One solver attempt, 90 seconds of browser work, and owned-browser cleanup on
success or failure. Blocked/banned/failed outcomes exit nonzero. `NotPresent` is
not reported as a solve. No page bodies, cookies, screenshots or storage are dumped.

Supported options:

| Variable | Purpose |
|---|---|
| `EOKA_TEST_VISIBLE=1` | Headed Chrome; use private `xvfb-run` without a display |
| `EOKA_TEST_NATIVE_LAUNCH=1` | Live-session launch without header stripping |
| `EOKA_TEST_CHROME_PATH` | Explicit browser binary; actual version is printed |
| `EOKA_TEST_PROFILE_DIR` | Dedicated private profile retained on close |
| `EOKA_TEST_SOFTWARE_COMPOSITING=1` | Opt-in Vulkan WebGL/software compositing for the tested Xvfb/NVIDIA host |
| `EOKA_TEST_CLICK_SELECTOR` | One hit-tested interaction before solving |
| `EOKA_TEST_REQUIRE_SOLVED=1` | Require a real challenge clear and subsequent navigation/content proof |
| `EOKA_TEST_SUCCESS_SELECTOR` | Required expected visible content for strict challenge proof |
| `EOKA_TEST_PROFILE_URL` | SoundCloud profile URL/slug; always verify its password stage |
| `EOKA_TEST_REQUIRE_PASSWORD_STAGE=1` | Explicit password-stage intent; required for credential mode |
| `EOKA_TEST_PASSWORD_FILE` | Opt-in credential entry, described below |

The example's `click_visible` helper selects one unambiguous visible target,
checks iframe owners for overlays, then clicks the exact checked point without
jitter. It rechecks the original element and coordinates after hover; replacements,
movement and covering elements abort rather than retargeting. Temporary element
references stay in an isolated world and expire if the operation is cancelled.
These are preflight checks, not an atomic guarantee against later page mutation.
The helper is example support code, not a public click API.

A profile identifier requires a SoundCloud target URL and defaults the initial
click to `button.loginButton`. The password-stage gate requires the secure auth
origin, matching read-only identifier and visible, hit-tested, empty current-
password input. It may repeat the initial identifier step once after an observed,
origin/source-checked challenge pass. It never retries credential submission.

`EOKA_TEST_REQUIRE_SOLVED` and `EOKA_TEST_SUCCESS_SELECTOR` must be supplied
together; an otherwise-unused success selector is rejected.

Strict challenge proof also requires the passed event for SoundCloud, followed
by navigation to the requested origin/path, expected content and no challenge
frame. A public homepage selector alone does not prove protected access.

Never use an everyday browser profile. Keep retained profiles paired with their
browser build; do not reopen a newer stable profile with an older beta binary.
Browser selection alone was not established as the cause of acceptance: both
beta and stable passed the final fresh-profile controls.

## Credential mode

Requires `EOKA_TEST_PASSWORD_FILE`, `EOKA_TEST_REQUIRE_PASSWORD_STAGE=1`,
`EOKA_TEST_PROFILE_URL` and a dedicated `EOKA_TEST_PROFILE_DIR`.

The Unix-only reader requires an owned, regular, single-link mode-0600 file. It
rejects final-component symlinks, special files, invalid UTF-8, control characters
and files exceeding 4096 bytes. One terminal LF/CRLF is removed; spaces remain.
The password is read only after the origin/account/empty-field gates pass.

Create the file on the machine running Eoka, without putting its contents in
command arguments or chat:

```bash
secret_file=$(mktemp /tmp/eoka-password.XXXXXX)
read -r -s -p 'SoundCloud password: ' secret
printf '%s' "$secret" > "$secret_file"
unset secret
printf '\nSecret-file path: %s\n' "$secret_file"
```

Pass only the path via `EOKA_TEST_PASSWORD_FILE`. Remove the file when finished;
the example does not delete caller-owned files automatically.

The verified field is tagged after filling so changes to its initial attributes
do not break submission. Guards retain the origin, account, password type,
visibility and single-submit requirements. Button hit tests accept child labels,
not unrelated overlays. Credential mode disables tracing and suppresses sensitive
failure details.

Success requires a post-submit GET 200 from `https://api-v2.soundcloud.com/me`, an
untruncated user record matching the authorized permalink and a positive user ID,
closure of the auth iframe, no remaining DataDome frame and the expected top-level
application origin. Identity capture is bounded, header-free and memory-only.
Pre-submit identity, additional CAPTCHA/MFA, missing `/me` traffic or incomplete
capture cannot establish a fresh login. Protect the retained profile as secret
material; authentication persistence after restart has not been verified.

## Tests

```sh
mise exec -- cargo test -p eoka-datadome --all-targets --all-features
mise exec -- cargo test -p eoka-datadome --test nested_fixture -- --ignored
mise exec -- cargo test -p eoka-datadome --example datadome_solve -- --ignored
mise exec -- cargo clippy -p eoka-datadome --all-targets --all-features -- -D warnings
mise exec -- cargo test -p eoka-cli --test datadome_cli -- --ignored
mise run conformance
mise run release-check
```

Chrome tests use local fixtures: nested image/simple sliders, delayed re-blocking,
password guards and a dummy credential POST followed by a `fixture-only` cookie
and authenticated identity check. The opt-in `test-fixtures` feature exposes the
shared synthetic fixture for CLI integration tests; it is not enabled for normal
CLI builds. CLI tests additionally verify owned/borrowed session continuity,
interception, no-op/block/ban/failure/timeout outcomes and client disconnect.
Fixture success is not live acceptance proof.

## Retired investigation code

The X11 input backend, no-CDP control, cookie tracing, broad response/body dumps,
native-NetLog switch and screenshot/state-export switches were removed from this
example after investigation. Their old environment flags now fail explicitly
rather than being silently ignored. Eoka's reusable response-capture and state
APIs remain intact.
