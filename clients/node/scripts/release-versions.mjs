// Release version sources of truth: the SDK version in clients/node/package.json
// and the server version in crates/eoka-server/Cargo.toml. The manifest,
// lockfile and readiness file repeat them because they ship or gate releases;
// scripts/bump-release.mjs writes them and versionDrift() checks them.
import { readFile } from 'node:fs/promises';
import { join, resolve } from 'node:path';

const sdkRoot = resolve(import.meta.dirname, '..');

export async function serverCrateVersion(repoRoot = resolve(sdkRoot, '../..')) {
  const cargo = await readFile(join(repoRoot, 'crates', 'eoka-server', 'Cargo.toml'), 'utf8');
  const version = /^version = "([^"]+)"$/m.exec(cargo)?.[1];
  if (!version) throw new Error('crates/eoka-server/Cargo.toml has no package version');
  return version;
}

export async function releaseVersions(root = sdkRoot) {
  const pkg = JSON.parse(await readFile(join(root, 'package.json'), 'utf8'));
  return { sdk: pkg.version, server: await serverCrateVersion(resolve(root, '../..')) };
}

export async function versionDrift(root = sdkRoot) {
  const { sdk, server } = await releaseVersions(root);
  const read = async file => JSON.parse(await readFile(join(root, file), 'utf8'));
  const manifest = await read('release-manifest.json');
  const lock = await read('package-lock.json');
  const readiness = await read('scripts/publication-readiness.json');
  const found = {
    'release-manifest.json sdkVersion': manifest.sdkVersion,
    ...Object.fromEntries(Object.entries(manifest.server.platforms).map(([id, pin]) => [`release-manifest.json ${id} packageVersion`, pin.packageVersion])),
    'package-lock.json version': lock.version,
    'package-lock.json root package version': lock.packages?.['']?.version,
    'publication-readiness.json sdkVersion': readiness.sdkVersion,
  };
  const drift = Object.entries(found).filter(([, value]) => value !== sdk).map(([where, value]) => `${where} is ${value}, package.json is ${sdk}`);
  if (manifest.server.version !== server) drift.push(`release-manifest.json server.version is ${manifest.server.version}, eoka-server crate is ${server}`);
  if (readiness.chromeBuildId !== manifest.chrome.buildId) drift.push(`publication-readiness.json chromeBuildId is ${readiness.chromeBuildId}, manifest pins ${manifest.chrome.buildId}`);
  return drift;
}
