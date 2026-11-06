// Install the release-manifest Chrome for Testing through the SDK installer
// (checksum-verified), and expose it to the Rust, Node and Go browser tests.
import { appendFile, cp, writeFile } from 'node:fs/promises';
import { basename, dirname, join } from 'node:path';
import { installBrowser } from '../../clients/node/dist/index.js';

const { executablePath } = await installBrowser({ browserCacheDir: process.env.EOKA_BROWSER_CACHE_DIR, timeoutMs: 300000 });
const rustChromeDirectory = join(process.env.RUNNER_TEMP, 'eoka-rust-chrome');
await cp(dirname(executablePath), rustChromeDirectory, { recursive: true });
const rustChrome = join(rustChromeDirectory, basename(executablePath));
await appendFile(process.env.GITHUB_ENV, `EOKA_RELIABILITY_CHROME=${executablePath}\nEOKA_TEST_CHROME_PATH=${rustChrome}\n`);
const launcher = join(rustChromeDirectory, 'google-chrome');
await writeFile(launcher, '#!/bin/sh\nexec "$(dirname "$(readlink -f "$0")")/chrome" "$@"\n', { mode: 0o755 });
await appendFile(process.env.GITHUB_PATH, `${dirname(launcher)}\n`);
