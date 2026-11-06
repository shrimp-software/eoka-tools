#!/usr/bin/env node
// Move the pinned Chrome for Testing build to the current Stable release.
//
// Rewrites the build id, download URLs and sha256 digests in the release
// manifest (hashing each platform archive as it streams) and the build id in
// the publication readiness file. Prints `updated=<bool>` and `version=<id>`
// in GITHUB_OUTPUT format.
//
//   node scripts/bump-chrome.mjs [--version <build>] [--manifest <path>] [--readiness <path>]
import { createHash } from 'node:crypto';
import { readFile, writeFile } from 'node:fs/promises';
import { parseArgs } from 'node:util';

const { values: args } = parseArgs({
  options: {
    version: { type: 'string' },
    manifest: { type: 'string', default: 'clients/node/release-manifest.json' },
    readiness: { type: 'string', default: 'clients/node/scripts/publication-readiness.json' },
  },
});

const compare = (a, b) => {
  const [x, y] = [a, b].map(v => v.split('.').map(Number));
  for (let i = 0; i < 4; i++) if (x[i] !== y[i]) return x[i] - y[i];
  return 0;
};

let target = args.version;
if (!target) {
  const response = await fetch('https://googlechromelabs.github.io/chrome-for-testing/last-known-good-versions.json');
  if (!response.ok) throw new Error(`Chrome for Testing versions: HTTP ${response.status}`);
  target = (await response.json()).channels.Stable.version;
}
if (!/^\d+\.\d+\.\d+\.\d+$/.test(target)) throw new Error(`Unexpected Chrome version: ${target}`);

let text = await readFile(args.manifest, 'utf8');
const manifest = JSON.parse(text);
const current = manifest.chrome.buildId;
if (!args.version && compare(target, current) <= 0) {
  console.log(`Chrome ${current} is current (Stable ${target}).`);
  console.log('updated=false');
  process.exit(0);
}

for (const [id, pin] of Object.entries(manifest.chrome.platforms)) {
  const url = pin.url.replace(`/${current}/`, `/${target}/`);
  const response = await fetch(url);
  if (!response.ok) throw new Error(`${id}: HTTP ${response.status} for ${url}`);
  const hash = createHash('sha256');
  for await (const chunk of response.body) hash.update(chunk);
  const digest = hash.digest('hex');
  console.error(`${id} ${digest} ${url}`);
  // Edit the text in place so the manifest keeps its one-line-per-platform layout.
  text = text.replace(pin.sha256, digest);
}
text = text.replaceAll(current, target);
const updated = JSON.parse(text);
if (updated.chrome.buildId !== target) throw new Error('Manifest rewrite failed');
await writeFile(args.manifest, text);

const readiness = await readFile(args.readiness, 'utf8');
await writeFile(args.readiness, readiness.replaceAll(current, target));

console.log('updated=true');
console.log(`version=${target}`);
console.log(`previous=${current}`);
