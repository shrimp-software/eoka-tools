// Release version sources of truth: the SDK version in clients/node/package.json,
// the server version in crates/eoka-server/Cargo.toml and the eoka version in
// the root Cargo.toml [workspace.dependencies]. The Node manifest, lockfile and
// readiness file, the eoka-server eoka pin and the Go server download repeat
// them; scripts/bump-release.mjs writes them, versionDrift() checks the Node
// files and repoVersionDrift() checks the rest.
import { readFile, writeFile } from 'node:fs/promises';
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

const repoRoot = resolve(sdkRoot, '../..');
const readRepo = path => readFile(join(repoRoot, path), 'utf8');

const repoPins = [
  { file: 'crates/eoka-server/Cargo.toml', source: 'eoka', pattern: /^(eoka = "=)([^"]+)(")$/gm },
  { file: 'clients/go/download.go', source: 'server', pattern: /^(const serverReleaseVersion = ")([^"]+)(")$/gm },
  { file: 'clients/go/README.md', source: 'server', pattern: /(eoka-server-v)(\d+\.\d+\.\d+[^/)\]`\s]*)()/g },
];

export function workspaceDependencyVersion(cargoToml, name) {
  const section = cargoToml.split(/^\[workspace\.dependencies\]$/m)[1]?.split(/^\[/m)[0] ?? '';
  const line = new RegExp(`^${name} = (.+)$`, 'm').exec(section)?.[1] ?? '';
  const version = /^"([^"]+)"$/.exec(line)?.[1] ?? /\bversion = "([^"]+)"/.exec(line)?.[1];
  if (!version) throw new Error(`Cargo.toml [workspace.dependencies] has no version for ${name}`);
  return version;
}

async function repoVersions() {
  return { eoka: workspaceDependencyVersion(await readRepo('Cargo.toml'), 'eoka'), server: await serverCrateVersion(repoRoot) };
}

export async function repoVersionDrift() {
  const versions = await repoVersions();
  const drift = [];
  for (const { file, source, pattern } of repoPins) {
    const found = [...(await readRepo(file)).matchAll(pattern)].map(match => match[2]);
    if (!found.length) drift.push(`${file} no longer contains its ${source} version pin`);
    for (const value of found) if (value !== versions[source]) drift.push(`${file} pins ${value}, ${source} is ${versions[source]}`);
  }
  return drift;
}

export async function writeRepoPins() {
  const versions = await repoVersions();
  for (const { file, source, pattern } of repoPins) {
    const text = await readRepo(file);
    await writeFile(join(repoRoot, file), text.replaceAll(pattern, (_, before, _old, after) => `${before}${versions[source]}${after}`));
  }
}
