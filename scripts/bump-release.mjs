#!/usr/bin/env node
// Set the one release version every crate and SDK shares, and write it into
// every file that repeats it.
//
//   node scripts/bump-release.mjs <version>   write the version everywhere
//   node scripts/bump-release.mjs --check     fail if any file drifted
import { releaseVersion, versionDrift, writeVersion } from '../clients/node/scripts/release-versions.mjs';

const [arg] = process.argv.slice(2);
if (arg !== '--check') {
  if (!/^\d+\.\d+\.\d+$/.test(arg ?? '')) throw new Error('Usage: node scripts/bump-release.mjs <major.minor.patch> | --check');
  await writeVersion(arg);
}
const drift = await versionDrift();
for (const line of drift) console.error(`::error::${line}`);
if (drift.length) process.exit(1);
console.log(arg === '--check' ? `Release version ${await releaseVersion()} is consistent.` : `Release version is now ${arg}.`);
