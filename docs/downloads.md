# Downloads and bounded fetch

Use an existing Eoka-owned session to save files with Chrome's cookies, proxy and
browser network stack:

```sh
eoka --session work open https://example.com/files
eoka --session work download --click 'text:Download' --out ./archive.zip
eoka --session work download list
eoka --session work download status JOB_ID
eoka --session work download wait JOB_ID --timeout-ms 120000
eoka --session work download cancel JOB_ID
eoka --session work download --url https://example.com/attachment --out ./file.zip --wait
```

Starts return a job ID; `--wait` adds a separate wait. Wait timeout or client
exit leaves the transfer running, and active jobs prevent idle shutdown. One file
may run per session. Closing the session cancels unfinished work. Jobs remain in
durable SQLite storage; daemon restart preserves job history. Recovery can reconcile
provably completed files. There is no
resume, queue or fallback HTTP downloader. Borrowed/remote `--cdp` browsers are
rejected. Existing old daemons need an explicit restart to gain these operations;
requests never upgrade or replay automatically.

`--click` accepts normal observation refs, text and CSS targets. `--url` accepts an
HTTP(S) attachment and uses a temporary tab in the same browser context, preserving
the selected tab. HTML pages do not count as successful file downloads. Defaults:
30 seconds to observe a native start (`--start-timeout-ms`, maximum 300000) and
30 seconds per wait (`--timeout-ms`, maximum 86400000). Neither limits an active
background transfer. `--expect-bytes` checks an independently known size.

The output's parent must exist. Relative CLI paths resolve against the caller's
working directory; SDK/MCP/Tack inputs require absolute filenames. A private
staging directory is created beside the destination. Completion checks the actual
regular file and byte counts, syncs it, then publishes through an atomic hard link
that cannot replace an existing file. Filesystems must support hard links.
Successful files survive session closure. Failed jobs report retained
`staging_path`, and `published_path` if publication occurred before a later cleanup
failure. These paths are local to the daemon/MCP host. Suggested filenames are
untrusted display metadata. Check status before retrying an ambiguous action.

`fetch` is text inspection: it streams at most 8192 raw bytes by default, cancels
at the limit, and reports `bytesRead`, `truncated` and `bodyOmitted`. `--max-body`
accepts 0 (headers only) through 16777216. Known binary MIME bodies are skipped;
use `download` to save them. `--body-only` still obeys the limit and fails if the
body is binary or truncated. An exact-limit body is conservatively truncated until
EOF is observed; incomplete trailing UTF-8 is omitted. Responses that expand beyond
the protocol budget after JSON escaping fail with guidance to lower the limit.
These bounds cover retained text, not Chrome's internal network buffering.

The canonical operations are `download.start`, `download.list`, `download.get`,
`download.wait` and `download.cancel`. SDK helpers use typed protocol arguments;
Tack exposes them as `eoka.download.*`, and native MCP uses the canonical dotted
names. Job/file policy lives in `eoka-server`; the sibling `eoka` library supplies
native event watching and bounded browser fetch primitives.

Local browser regression (also runs with `EOKA_DOWNLOAD_TEST_HEADED=1 xvfb-run -d`
and optionally `EOKA_DOWNLOAD_TEST_LIVE=1`):

```sh
cargo test -p eoka-cli --test download_cli -- --ignored --test-threads=1
```

Concurrent headed runs need separate displays. Use `xvfb-run -d` where supported;
this asks Xvfb to allocate the display. If the wrapper only supports `-a`, run the
headed suites sequentially: some wrappers race when choosing a free display, then
continue running tests even when their X server failed to start.
