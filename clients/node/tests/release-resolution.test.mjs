import assert from 'node:assert/strict';
import { execFile } from 'node:child_process';
import { createHash } from 'node:crypto';
import { chmod, cp, mkdir, mkdtemp, readFile, rm, symlink, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { test } from 'node:test';
import { promisify } from 'node:util';
import { detectPlatform, platformIds, selectPlatform } from '../dist/platform.js';
import { releaseManifest, validateReleaseManifest } from '../dist/release.js';
import { resolveServerBinary } from '../dist/server-binary.js';

const exec = promisify(execFile);

for (const [os, arch, libc, expected] of [
  ['linux', 'x64', '2.35', 'linux-x64-gnu'], ['linux', 'arm64', '2.36', 'linux-arm64-gnu'],
  ['darwin', 'x64', undefined, 'darwin-x64'], ['darwin', 'arm64', undefined, 'darwin-arm64'],
  ['win32', 'x64', undefined, 'win32-x64'],
]) {
  test(`platform selects ${expected}`, () => assert.equal(selectPlatform(os, arch, libc), expected));
}

for (const [os, arch, libc] of [
  ['linux', 'x64', undefined], ['linux', 'x64', 'musl'], ['linux', 'arm64', '2.34'],
  ['linux', 'x64', '2.9'], ['linux', 'ia32', '2.40'], ['win32', 'arm64'], ['freebsd', 'x64'],
]) {
  test(`platform rejects ${os}/${arch}/${libc}`, () => assert.throws(() => selectPlatform(os, arch, libc), { code: 'UnsupportedPlatform' }));
}

test('manifest contains immutable exact pins and explicitly unstaged servers', () => {
  for (const id of platformIds) {
    assert.match(releaseManifest.chrome.platforms[id].sha256, /^[a-f0-9]{64}$/);
    assert.equal(releaseManifest.server.platforms[id].packageVersion, releaseManifest.sdkVersion);
    assert.equal(releaseManifest.server.platforms[id].sha256, null);
    assert.ok(Object.isFrozen(releaseManifest.chrome.platforms[id]));
  }
});

test('manifest rejects mutable URLs, incomplete or malformed pins and dishonest ABI claims', () => {
  for (const mutate of [
    (manifest) => { manifest.chrome.platforms['linux-x64-gnu'].url = 'https://example.com/latest.zip'; },
    (manifest) => { manifest.chrome.platforms['linux-x64-gnu'].sha256 = null; },
    (manifest) => { manifest.chrome.platforms['darwin-arm64'].executablePath = '../chrome'; },
    (manifest) => { manifest.server.platforms['linux-x64-gnu'].sha256 = '0'.repeat(64); },
    (manifest) => { manifest.server.platforms['linux-x64-gnu'].packageVersion = `^${manifest.sdkVersion}`; },
    (manifest) => { manifest.server.platforms['linux-x64-gnu'].minimumGlibc = '2.39'; },
    (manifest) => { delete manifest.server.platforms['win32-x64']; },
    (manifest) => { manifest.server.protocolVersion = '1.4'; },
  ]) {
    const manifest = structuredClone(releaseManifest);
    mutate(manifest);
    assert.throws(() => validateReleaseManifest(manifest), { code: 'ReleaseNotPrepared' });
  }
});

for (const version of ['0.1.0-beta.1', '1.0.0-rc.0', '1.2.3-alpha-beta.2+build.001', '1.2.3']) {
  test(`manifest accepts exact npm version ${version}`, () => {
    const manifest = structuredClone(releaseManifest);
    manifest.sdkVersion = version;
    for (const pin of Object.values(manifest.server.platforms)) pin.packageVersion = version;
    assert.equal(validateReleaseManifest(manifest).sdkVersion, version);
  });
}

for (const version of ['9007199254740992.0.0', '0.9007199254740992.0', '0.0.9007199254740992', `1.2.3-${'a'.repeat(251)}`]) {
  test(`manifest rejects an npm version outside parser limits ${JSON.stringify(version)}`, () => {
    const manifest = structuredClone(releaseManifest);
    manifest.sdkVersion = version;
    for (const pin of Object.values(manifest.server.platforms)) pin.packageVersion = version;
    assert.throws(() => validateReleaseManifest(manifest), { code: 'ReleaseNotPrepared' });
  });
}

for (const version of ['latest', 'next', '^0.1.0-beta.1', '~1.2.3', '1.2', '1.2.x', 'v1.2.3', '01.2.3', '1.02.3', '1.2.03', '1.2.3-beta.01', '1.2.3-', '1.2.3-beta..1', '1.2.3+', '1.2.3+build..1', '1.2.3\n', ' 1.2.3']) {
  test(`manifest rejects invalid or floating npm version ${JSON.stringify(version)}`, () => {
    const manifest = structuredClone(releaseManifest);
    manifest.sdkVersion = version;
    for (const pin of Object.values(manifest.server.platforms)) pin.packageVersion = version;
    assert.throws(() => validateReleaseManifest(manifest), { code: 'ReleaseNotPrepared' });
  });
}

test('explicit server overrides environment and remains an expert platform bypass', async () => {
  const directory = await mkdtemp(join(tmpdir(), 'eoka-server-override-'));
  try {
    const binary = join(directory, 'server with spaces');
    await writeFile(binary, 'fixture executable');
    await chmod(binary, 0o755);
    assert.deepEqual(await resolveServerBinary({ serverPath: binary, env: { EOKA_SERVER_BIN: join(directory, 'missing') } }), { executablePath: binary, source: 'explicit' });
    assert.deepEqual(await resolveServerBinary({ env: { EOKA_SERVER_BIN: binary } }), { executablePath: binary, source: 'environment' });
    await assert.rejects(resolveServerBinary({ serverPath: '' }), { code: 'Configuration' });
    await assert.rejects(resolveServerBinary({ serverPath: directory }), { code: 'Configuration' });
    const controller = new AbortController();
    controller.abort();
    await assert.rejects(resolveServerBinary({ serverPath: binary, signal: controller.signal }), { code: 'InstallationAborted' });
  } finally {
    await rm(directory, { recursive: true, force: true });
  }
});

async function fixture(run) {
  const directory = await mkdtemp(join(tmpdir(), 'eoka release consumer '));
  try {
    const platform = detectPlatform();
    const manifest = structuredClone(releaseManifest);
    const pin = manifest.server.platforms[platform];
    const contents = Buffer.from('pinned package executable integrity fixture');
    pin.sha256 = createHash('sha256').update(contents).digest('hex');
    pin.size = contents.length;
    await cp(new URL('../dist/', import.meta.url), join(directory, 'dist'), { recursive: true });
    await writeFile(join(directory, 'package.json'), '{"type":"module"}');
    await writeFile(join(directory, 'dist', 'release-manifest.json'), JSON.stringify(manifest));
    const packageRoot = join(directory, 'node_modules', pin.packageName);
    await mkdir(join(packageRoot, 'bin'), { recursive: true });
    const [os, arch] = platform.split('-');
    const packageJson = { name: pin.packageName, version: pin.packageVersion, os: [os], cpu: [arch], ...(os === 'linux' ? { libc: ['glibc'] } : {}) };
    const metadata = {
      schemaVersion: 1, sdkVersion: manifest.sdkVersion, packageName: pin.packageName, packageVersion: pin.packageVersion,
      platform, target: pin.target, serverVersion: manifest.server.version, protocolVersion: manifest.server.protocolVersion,
      minimumGlibc: pin.minimumGlibc, development: pin.development, binary: { path: pin.executablePath, sha256: pin.sha256, size: pin.size },
    };
    const binary = join(packageRoot, pin.executablePath);
    await writeFile(binary, contents);
    await chmod(binary, 0o755);
    await writeFile(join(packageRoot, 'package.json'), JSON.stringify(packageJson));
    await writeFile(join(packageRoot, 'release-metadata.json'), JSON.stringify(metadata));
    const probe = async () => {
      const result = await exec(process.execPath, ['--input-type=module', '--eval', `const {resolveServerBinary}=await import('./dist/server-binary.js');try{console.log(JSON.stringify(await resolveServerBinary({env:{}})))}catch(error){console.log(JSON.stringify({code:error.code,message:error.message}))}`], { cwd: directory, env: { ...process.env, PATH: '' } });
      assert.equal(result.stderr, '');
      return JSON.parse(result.stdout);
    };
    await run({ directory, packageRoot, manifest, pin, metadata, packageJson, binary, contents, probe });
  } finally {
    await rm(directory, { recursive: true, force: true });
  }
}

test('external consumer resolves only its exact pinned optional package without PATH', () => fixture(async ({ binary, manifest, probe }) => {
  assert.deepEqual(await probe(), { executablePath: binary, source: 'package', expectedServerVersion: manifest.server.version });
}));

test('missing optional package is actionable even for source-only unstaged SDK', () => fixture(async ({ packageRoot, directory, manifest, pin, probe }) => {
  pin.sha256 = null;
  pin.size = null;
  await writeFile(join(directory, 'dist', 'release-manifest.json'), JSON.stringify(manifest));
  await rm(packageRoot, { recursive: true });
  const result = await probe();
  assert.equal(result.code, 'ServerPackageMissing');
  assert.match(result.message, /optional dependencies enabled/);
  assert.match(result.message, /no Cargo, PATH or network fallback/);
}));

test('unstaged SDK never trusts a package to authorize its own digest', () => fixture(async ({ directory, manifest, pin, probe }) => {
  pin.sha256 = null;
  pin.size = null;
  await writeFile(join(directory, 'dist', 'release-manifest.json'), JSON.stringify(manifest));
  assert.equal((await probe()).code, 'ReleaseNotPrepared');
}));

test('same-size corruption fails against SDK digest', () => fixture(async ({ binary, contents, probe }) => {
  contents[0] ^= 0xff;
  await writeFile(binary, contents);
  assert.equal((await probe()).code, 'IntegrityMismatch');
}));

for (const field of ['packageVersion', 'serverVersion', 'protocolVersion', 'platform', 'target', 'minimumGlibc']) {
  test(`platform metadata rejects mismatched ${field}`, () => fixture(async ({ packageRoot, metadata, probe }) => {
    metadata[field] = 'incorrect';
    await writeFile(join(packageRoot, 'release-metadata.json'), JSON.stringify(metadata));
    assert.equal((await probe()).code, 'ServerPackageMismatch');
  }));
}

test('floating or wrong package versions fail closed', () => fixture(async ({ packageRoot, packageJson, probe }) => {
  packageJson.version = '0.0.1';
  await writeFile(join(packageRoot, 'package.json'), JSON.stringify(packageJson));
  assert.equal((await probe()).code, 'ServerPackageMismatch');
}));

for (const field of ['os', 'cpu', 'libc']) {
  test(`platform package rejects malformed or mismatched ${field} declarations`, { skip: field === 'libc' && process.platform !== 'linux' }, () => fixture(async ({ packageRoot, packageJson, probe }) => {
    const expected = packageJson[field][0];
    for (const value of [expected, [expected, expected], { 0: expected, length: 1 }, [`${expected}-incorrect`]]) {
      packageJson[field] = value;
      await writeFile(join(packageRoot, 'package.json'), JSON.stringify(packageJson));
      assert.equal((await probe()).code, 'ServerPackageMismatch');
    }
  }));
}

test('binary symlink escape fails closed', { skip: process.platform === 'win32' }, () => fixture(async ({ directory, binary, contents, probe }) => {
  const outside = join(directory, 'outside');
  await writeFile(outside, contents);
  await chmod(outside, 0o755);
  await rm(binary);
  await symlink(outside, binary);
  assert.equal((await probe()).code, 'ServerPackageMismatch');
}));

test('group/world-writable package executable fails closed', { skip: process.platform === 'win32' }, () => fixture(async ({ binary, probe }) => {
  await chmod(binary, 0o777);
  assert.equal((await probe()).code, 'ServerPackageMismatch');
}));
