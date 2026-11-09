#!/usr/bin/env node
// Set the Node SDK release version and propagate it, the eoka-server crate
// version and the workspace eoka version into every file that repeats them.
//
//   node scripts/bump-release.mjs [--sdk <version>]   write the derived files
//   node scripts/bump-release.mjs --check             fail if any file drifted
import { readFile, writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import { parseArgs } from 'node:util';
import { releaseVersions, repoVersionDrift, versionDrift, writeRepoPins } from '../clients/node/scripts/release-versions.mjs';

const { values: args } = parseArgs({ options: { sdk: { type: 'string' }, check: { type: 'boolean', default: false } } });
const root = join(import.meta.dirname, '..', 'clients', 'node');
const path = file => join(root, file);
const writeJson = (file, value) => writeFile(path(file), `${JSON.stringify(value, null, 2)}\n`);

if (args.check) {
  const drift = [...(await versionDrift(root)), ...(await repoVersionDrift())];
  for (const line of drift) console.error(`::error::${line}`);
  if (drift.length) process.exit(1);
  console.log('Release versions are consistent.');
  process.exit(0);
}

if (args.sdk) {
  const pkg = JSON.parse(await readFile(path('package.json'), 'utf8'));
  pkg.version = args.sdk;
  await writeJson('package.json', pkg);
}
const { sdk, server } = await releaseVersions(root);

const lock = JSON.parse(await readFile(path('package-lock.json'), 'utf8'));
lock.version = sdk;
lock.packages[''].version = sdk;
await writeJson('package-lock.json', lock);

// Edit the manifest text in place to keep its one-line-per-platform layout.
let manifest = await readFile(path('release-manifest.json'), 'utf8');
const current = JSON.parse(manifest);
manifest = manifest
  .replace(`"sdkVersion": "${current.sdkVersion}"`, `"sdkVersion": "${sdk}"`)
  .replaceAll(`"packageVersion": "${current.sdkVersion}"`, `"packageVersion": "${sdk}"`)
  .replace(`"version": "${current.server.version}"`, `"version": "${server}"`);
await writeFile(path('release-manifest.json'), manifest);

const readiness = JSON.parse(await readFile(path('scripts/publication-readiness.json'), 'utf8'));
readiness.sdkVersion = sdk;
await writeJson('scripts/publication-readiness.json', readiness);

await writeRepoPins();

const drift = [...(await versionDrift(root)), ...(await repoVersionDrift())];
if (drift.length) throw new Error(`Still inconsistent:\n${drift.join('\n')}`);
console.log(`SDK ${sdk}, server ${server}`);
