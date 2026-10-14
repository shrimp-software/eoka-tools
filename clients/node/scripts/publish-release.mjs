import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { execFile } from 'node:child_process';
import { createReadStream } from 'node:fs';
import { lstat, mkdir, mkdtemp, readFile, readdir, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { basename, join, resolve } from 'node:path';
import { promisify } from 'node:util';
import { resumePublication, verifyExistingPackage } from './publication-recovery.mjs';
import { assertRecordingQualification } from './recording-qualification.mjs';

const exec = promisify(execFile);
const root = resolve(import.meta.dirname, '..');
const repository = 'shrimp-software/eoka-tools';
const registry = 'https://registry.npmjs.org/';
const platforms = ['linux-x64-gnu', 'linux-arm64-gnu', 'darwin-x64', 'darwin-arm64', 'win32-x64'];
const payload = resolve(process.env.QUALIFICATION_DIRECTORY ?? join(process.env.RUNNER_TEMP ?? root, 'qualified-release'));
const sha = process.env.SOURCE_SHA;
const json = async path => JSON.parse(await readFile(path, 'utf8'));
const hash = async (path, algorithm = 'sha256', encoding = 'hex') => {
  const digest = createHash(algorithm);
  for await (const chunk of createReadStream(path)) digest.update(chunk);
  return digest.digest(encoding);
};

async function api(path) {
  const response = await fetch(`https://api.github.com/repos/${repository}/${path}`, {
    headers: { Authorization: `Bearer ${process.env.GH_TOKEN}`, Accept: 'application/vnd.github+json', 'X-GitHub-Api-Version': '2022-11-28' },
  });
  if (!response.ok) throw new Error(`GitHub ${path} failed: ${response.status}`);
  return response;
}

async function sourceGate() {
  assert.match(sha ?? '', /^[a-f0-9]{40}$/);
  assert.equal(process.version, 'v24.21.0');
  assert.equal(process.env.GITHUB_REPOSITORY, repository);
  assert.equal(process.env.GITHUB_EVENT_NAME, 'workflow_dispatch');
  assert.equal(process.env.GITHUB_REF, 'refs/heads/main');
  assert.equal(process.env.GITHUB_REF_PROTECTED, 'true');
  assert.equal(process.env.GITHUB_WORKFLOW_REF, `${repository}/.github/workflows/publish-node.yml@refs/heads/main`);
  assert.equal(process.env.GITHUB_SHA, sha);
  assert.equal((await exec('git', ['rev-parse', 'HEAD'], { cwd: root })).stdout.trim(), sha);
  assert.ok(process.env.GH_TOKEN, 'GitHub read token is required for source provenance');
  const branch = await (await api('branches/main')).json();
  assert.equal(branch.protected, true);
  assert.equal(branch.commit.sha, sha, 'Only the current exact protected main commit may publish');
  const environment = await (await api('environments/npm-prerelease')).json();
  const reviewers = environment.protection_rules?.find(rule => rule.type === 'required_reviewers');
  assert.ok(reviewers?.reviewers?.length > 0, 'npm-prerelease must already require reviewer approval');
  assert.equal(reviewers.prevent_self_review, true, 'npm-prerelease must prevent self-review');
  assert.equal(environment.deployment_branch_policy?.protected_branches, false);
  assert.equal(environment.deployment_branch_policy?.custom_branch_policies, true);
  const policy = await (await api('environments/npm-prerelease/deployment-branch-policies?per_page=100')).json();
  assert.equal(policy.total_count, 1, 'npm-prerelease must allow only main');
  assert.equal(policy.branch_policies[0]?.name, 'main');
  assert.equal(policy.branch_policies[0]?.type, 'branch');
  const manifest = await json(join(root, 'release-manifest.json'));
  const metadata = await json(join(root, 'package.json'));
  assert.equal(manifest.sdkVersion, '0.1.0-beta.1');
  assert.equal(metadata.name, '@cbxss/eoka');
  assert.equal(metadata.version, manifest.sdkVersion);
  assert.equal(metadata.private, true);
  assert.equal(metadata.optionalDependencies, undefined);
  assert.equal(metadata.engines.node, '^24.21.0');
  assert.equal(manifest.server.version, '0.5.0');
  assert.equal(manifest.server.protocolVersion, '1.3');
  const readiness = await json(join(root, 'scripts', 'publication-readiness.json'));
  assert.equal(readiness.schemaVersion, 1);
  assert.equal(readiness.sdkVersion, manifest.sdkVersion);
  assert.equal(readiness.chromeBuildId, manifest.chrome.buildId);
  assert.ok(readiness.status === 'ready' && Array.isArray(readiness.blockers) && readiness.blockers.length === 0, `Publication readiness blocked: ${JSON.stringify(readiness.blockers)}`);
  assert.deepEqual(Object.keys(manifest.server.platforms).sort(), [...platforms].sort());
  for (const id of platforms) {
    const pin = manifest.server.platforms[id];
    assert.equal(pin.packageName, `@cbxss/eoka-server-${id}`);
    assert.equal(pin.packageVersion, metadata.version);
    assert.equal(pin.development, false);
    assert.equal(pin.sha256, null);
    assert.equal(pin.size, null);
  }
  for (const [name, value] of Object.entries(process.env)) {
    if (value && /^(?:EOKA_(?:CORE_(?:REF|PATH)|SERVER_BIN|CHROME_PATH)|CARGO_PATCH_|CARGO_SOURCE_|CARGO_BUILD_RUSTFLAGS$|RUSTFLAGS$|NODE_AUTH_TOKEN$|NPM_TOKEN$|NPM_AUTH_TOKEN$|npm_config_.*auth|NPM_CONFIG_.*AUTH)/.test(name)) throw new Error(`Publication override or token is forbidden: ${name}`);
  }
  return { manifest, metadata };
}

function safePath(path) {
  assert.ok(typeof path === 'string' && path && !path.includes('\\') && !path.startsWith('/') && !path.split('/').some(part => part === '..' || part === '.' || !part), `Unsafe archive path: ${path}`);
  return path;
}

async function regularTree(directory) {
  for (const name of await readdir(directory)) {
    const path = join(directory, name);
    const stat = await lstat(path);
    assert.ok(stat.isFile() || stat.isDirectory(), `Nonregular qualification payload: ${path}`);
    if (stat.isDirectory()) await regularTree(path);
  }
}

async function validateZip(archive) {
  const { default: yauzl } = await import('yauzl');
  await new Promise((accept, reject) => {
    yauzl.open(archive, { lazyEntries: true, strictFileNames: true }, (error, zip) => {
      if (error) return reject(error);
      const names = new Set();
      zip.on('error', reject);
      zip.on('end', accept);
      zip.on('entry', entry => {
        try {
          const directory = entry.fileName.endsWith('/');
          safePath(directory ? entry.fileName.slice(0, -1) : entry.fileName);
          assert.ok(!names.has(entry.fileName), `Duplicate ZIP entry: ${entry.fileName}`);
          names.add(entry.fileName);
          const type = (entry.externalFileAttributes >>> 16) & 0xf000;
          assert.ok(type === 0 || type === 0x8000 || type === 0x4000, `ZIP links and special files are forbidden: ${entry.fileName}`);
          assert.ok(type !== 0x8000 || !directory, `ZIP file marked as a directory: ${entry.fileName}`);
          assert.ok(type !== 0x4000 || directory, `ZIP directory missing directory marker: ${entry.fileName}`);
          assert.ok(!(entry.externalFileAttributes & 0x08), `ZIP volume labels are forbidden: ${entry.fileName}`);
          assert.ok(!(entry.externalFileAttributes & 0x10) || directory, `ZIP DOS directory missing directory marker: ${entry.fileName}`);
          zip.readEntry();
        } catch (cause) {
          zip.close();
          reject(cause);
        }
      });
      zip.readEntry();
    });
  });
}

async function download() {
  await sourceGate();
  const id = process.env.QUALIFICATION_ARTIFACT_ID;
  const digest = process.env.QUALIFICATION_ARTIFACT_DIGEST?.replace(/^sha256:/, '');
  assert.match(id ?? '', /^[1-9][0-9]*$/);
  assert.match(digest ?? '', /^[a-f0-9]{64}$/);
  const artifact = await (await api(`actions/artifacts/${id}`)).json();
  assert.equal(artifact.name, 'eoka-npm-packages');
  assert.equal(artifact.expired, false);
  assert.equal(artifact.workflow_run.id, Number(process.env.GITHUB_RUN_ID));
  assert.equal(artifact.workflow_run.head_sha, sha);
  assert.equal(artifact.workflow_run.head_branch, 'main');
  assert.equal(artifact.digest, `sha256:${digest}`);
  const run = await (await api(`actions/runs/${process.env.GITHUB_RUN_ID}`)).json();
  assert.equal(run.repository.full_name, repository);
  assert.equal(run.head_sha, sha);
  assert.equal(run.event, 'workflow_dispatch');
  assert.equal(run.path, '.github/workflows/publish-node.yml');
  const temporary = await mkdtemp(join(tmpdir(), 'eoka-payload-'));
  try {
    const archive = join(temporary, 'qualification.zip');
    const response = await api(`actions/artifacts/${id}/zip`);
    const handle = await import('node:stream/promises');
    const fs = await import('node:fs');
    await handle.pipeline(response.body, fs.createWriteStream(archive));
    assert.equal(await hash(archive), digest, 'Qualification ZIP digest mismatch');
    await validateZip(archive);
    await mkdir(payload);
    await exec('unzip', ['-q', archive, '-d', payload]);
    await regularTree(payload);
    await verify();
  } finally {
    await rm(temporary, { recursive: true, force: true });
  }
}

async function unpack(path, directory) {
  const listing = await exec('tar', ['-tzf', path], { maxBuffer: 16 * 1024 * 1024 });
  const entries = listing.stdout.trim().split('\n');
  assert.equal(new Set(entries).size, entries.length);
  for (const entry of entries) {
    safePath(entry.endsWith('/') ? entry.slice(0, -1) : entry);
    assert.ok(entry.startsWith('package/'));
  }
  const verbose = await exec('tar', ['-tvzf', path], { maxBuffer: 16 * 1024 * 1024 });
  assert.ok(verbose.stdout.trim().split('\n').every(line => /^[-d]/.test(line)), 'Package links and special files are forbidden');
  await mkdir(directory);
  await exec('tar', ['-xzf', path, '-C', directory, '--no-same-owner']);
  await regularTree(directory);
  return join(directory, 'package');
}

async function verify() {
  const { manifest: source, metadata: sourceMetadata } = await sourceGate();
  const { validateReleaseManifest } = await import('../dist/release.js');
  const qualification = await json(join(payload, 'qualification.json'));
  assert.equal(qualification.schemaVersion, 1);
  assert.equal(qualification.sourceSha, sha);
  assert.equal(qualification.coreRef, null);
  assert.equal(qualification.publication, true);
  assert.equal(qualification.sdkVersion, source.sdkVersion);
  assert.equal(qualification.serverVersion, source.server.version);
  assert.equal(qualification.protocolVersion, source.server.protocolVersion);
  assert.deepEqual([...qualification.platforms].sort(), [...platforms].sort());
  assert.equal(qualification.evidence.length, 5);
  assert.deepEqual(qualification.evidence.map(item => item.platform).sort(), [...platforms].sort());
  for (const item of qualification.evidence) {
    assert.equal(item.filename, `evidence/${item.platform}/qualification.json`);
    const path = join(payload, safePath(item.filename));
    assert.match(item.sha256, /^[a-f0-9]{64}$/);
    assert.equal(await hash(path), item.sha256);
    const evidence = await json(path);
    assert.equal(evidence.platform, item.platform);
    assert.equal(evidence.sourceSha, sha);
    assert.equal(evidence.status, 'passed');
    assertRecordingQualification(evidence, source.chrome.buildId);
  }
  const expectedNames = ['@cbxss/eoka', ...platforms.map(id => source.server.platforms[id].packageName)];
  assert.equal(qualification.packages.length, 6);
  assert.deepEqual(qualification.packages.map(item => item.name).sort(), expectedNames.sort());
  assert.equal(new Set(qualification.packages.map(item => item.filename)).size, 6);
  assert.deepEqual((await readdir(payload)).filter(name => name.endsWith('.tgz')).sort(), qualification.packages.map(item => item.filename).sort());
  const temporary = await mkdtemp(join(tmpdir(), 'eoka-verify-'));
  try {
    const extracted = new Map();
    for (const item of qualification.packages) {
      assert.equal(item.version, source.sdkVersion);
      assert.equal(item.filename, `${item.name.replace(/^@/, '').replace('/', '-')}-${item.version}.tgz`);
      assert.equal(basename(item.filename), item.filename);
      assert.match(item.sha256, /^[a-f0-9]{64}$/);
      const archive = join(payload, item.filename);
      assert.equal(await hash(archive), item.sha256);
      const directory = await unpack(archive, join(temporary, item.name.replace(/[@/]/g, '-')));
      const metadata = await json(join(directory, 'package.json'));
      assert.equal(metadata.name, item.name);
      assert.equal(metadata.version, item.version);
      assert.equal(metadata.private, false);
      assert.equal(metadata.engines.node, sourceMetadata.engines.node);
      assert.equal(metadata.repository.url, `git+https://github.com/${repository}.git`);
      assert.ok(!metadata.scripts || Object.keys(metadata.scripts).every(key => !['preinstall', 'install', 'postinstall', 'prepublish', 'prepublishOnly', 'publish', 'postpublish', 'prepare'].includes(key)), 'Publication lifecycle scripts are forbidden');
      extracted.set(item.name, { directory, metadata });
    }
    const sdk = extracted.get('@cbxss/eoka');
    const prepared = validateReleaseManifest(await json(join(sdk.directory, 'dist', 'release-manifest.json')));
    const expected = structuredClone(source);
    for (const id of platforms) {
      const pin = prepared.server.platforms[id];
      const native = extracted.get(pin.packageName);
      const metadata = await json(join(native.directory, 'release-metadata.json'));
      assert.equal(pin.development, false);
      assert.match(pin.sha256 ?? '', /^[a-f0-9]{64}$/);
      assert.ok(Number.isSafeInteger(pin.size) && pin.size >= 64);
      assert.equal(metadata.schemaVersion, 1);
      assert.equal(metadata.sdkVersion, prepared.sdkVersion);
      assert.equal(metadata.packageName, pin.packageName);
      assert.equal(metadata.packageVersion, pin.packageVersion);
      assert.equal(metadata.platform, id);
      assert.equal(metadata.target, pin.target);
      assert.equal(metadata.serverVersion, source.server.version);
      assert.equal(metadata.protocolVersion, source.server.protocolVersion);
      assert.equal(metadata.development, false);
      assert.equal(metadata.minimumGlibc, pin.minimumGlibc);
      assert.deepEqual(metadata.binary, { path: pin.executablePath, sha256: pin.sha256, size: pin.size });
      assert.deepEqual(native.metadata.os, [id.split('-')[0]]);
      assert.deepEqual(native.metadata.cpu, [id.split('-')[1]]);
      if (id.startsWith('linux-')) {
        assert.deepEqual(native.metadata.libc, ['glibc']);
        assert.equal(pin.minimumGlibc, '2.35');
      }
      const binary = join(native.directory, safePath(pin.executablePath));
      assert.equal((await lstat(binary)).size, pin.size);
      assert.equal(await hash(binary), pin.sha256);
      expected.server.platforms[id].sha256 = pin.sha256;
      expected.server.platforms[id].size = pin.size;
    }
    assert.deepEqual(prepared, expected);
    const expectedMetadata = structuredClone(sourceMetadata);
    expectedMetadata.private = false;
    expectedMetadata.repository = { type: 'git', url: `git+https://github.com/${repository}.git`, directory: 'clients/node' };
    expectedMetadata.optionalDependencies = Object.fromEntries(platforms.map(id => [source.server.platforms[id].packageName, source.sdkVersion]));
    assert.deepEqual(sdk.metadata, expectedMetadata);
  } finally {
    await rm(temporary, { recursive: true, force: true });
  }
  return qualification;
}

async function npmEnvironment(directory) {
  const userconfig = join(directory, 'npmrc');
  const globalconfig = join(directory, 'global-npmrc');
  await writeFile(userconfig, '');
  await writeFile(globalconfig, '');
  const env = { ...process.env };
  for (const key of Object.keys(env)) if (/^npm_config_/i.test(key)) delete env[key];
  return { ...env, NPM_CONFIG_USERCONFIG: userconfig, NPM_CONFIG_GLOBALCONFIG: globalconfig, NPM_CONFIG_REGISTRY: registry, NPM_CONFIG_CACHE: join(directory, 'npm-cache'), NPM_CONFIG_FETCH_RETRIES: '0' };
}

async function registryBoundary(directory, env) {
  const readJson = async url => {
    const response = await fetch(url, { headers: { Accept: 'application/json', 'Cache-Control': 'no-cache' }, redirect: 'error', signal: AbortSignal.timeout(120000) });
    assert.equal(response.status, 200, `Registry read failed; existing scoped packages and trusted publishers are required (${response.status}): ${url}`);
    return response.json();
  };
  let verifier;
  const loadVerifier = async () => {
    const sigstore = await import('sigstore');
    assert.equal((await json(join(root, 'package.json'))).devDependencies.sigstore, '4.1.1', 'The fixed public Sigstore verifier must be pinned');
    const verifySignatures = async item => {
      const consumer = join(directory, `signature-${item.name.replace(/[@/]/g, '-')}`);
      await mkdir(consumer);
      await writeFile(join(consumer, 'package.json'), JSON.stringify({ name: 'eoka-signature-verification', version: '1.0.0', private: true }));
      await exec('npm', ['install', `${item.name}@${item.version}`, '--ignore-scripts', '--omit=optional', '--no-audit', '--no-fund', '--force', '--registry', registry, '--fetch-retries', '0'], { cwd: consumer, env, timeout: 180000, maxBuffer: 8 * 1024 * 1024 });
      const lock = await json(join(consumer, 'package-lock.json'));
      assert.equal(lock.packages?.[`node_modules/${item.name}`]?.integrity, item.integrity, `Installed registry tarball differs from qualification: ${item.name}`);
      const installed = await json(join(consumer, 'node_modules', item.name, 'package.json'));
      assert.equal(installed.name, item.name);
      assert.equal(installed.version, item.version);
      const { stdout } = await exec('npm', ['audit', 'signatures', '--json', '--ignore-scripts', '--registry', registry, '--fetch-retries', '0'], { cwd: consumer, env, timeout: 180000, maxBuffer: 8 * 1024 * 1024 });
      const result = JSON.parse(stdout);
      assert.deepEqual(result.invalid, []);
      assert.deepEqual(result.missing, []);
    };
    return {
      verifySignatures,
      sigstore,
      options: { registry, tufCache: join(directory, 'tuf-cache') },
      readJson,
      identity: { repository, sourceSha: sha, workflowPath: '.github/workflows/publish-node.yml', runId: process.env.GITHUB_RUN_ID },
    };
  };
  return {
    readPackage: name => readJson(`${registry}${encodeURIComponent(name)}`),
    verifyExisting: async (item, metadata) => {
      verifier ??= loadVerifier();
      await verifyExistingPackage(item, metadata, await verifier);
    },
    publish: async item => {
      const result = await exec('npm', ['publish', join(payload, item.filename), '--ignore-scripts', '--access', 'public', '--tag', 'next', '--provenance', '--registry', registry, '--fetch-retries', '0'], { cwd: directory, env, timeout: 180000, maxBuffer: 8 * 1024 * 1024 });
      process.stdout.write(result.stdout);
      process.stderr.write(result.stderr);
    },
  };
}

async function recoveryPackages(qualification) {
  return Promise.all(qualification.packages.map(async item => ({
    ...item,
    integrity: `sha512-${await hash(join(payload, item.filename), 'sha512', 'base64')}`,
  })));
}

async function publish() {
  const qualification = await verify();
  assert.ok(process.env.ACTIONS_ID_TOKEN_REQUEST_URL && process.env.ACTIONS_ID_TOKEN_REQUEST_TOKEN, 'Trusted OIDC is required; token fallback is forbidden');
  assert.match(process.env.GITHUB_RUN_ID ?? '', /^[1-9][0-9]*$/);
  const temporary = await mkdtemp(join(tmpdir(), 'eoka-publish-'));
  try {
    const env = await npmEnvironment(temporary);
    const { stdout } = await exec('npm', ['--version'], { cwd: temporary, env });
    const [major, minor, patch] = stdout.trim().split('.').map(Number);
    assert.ok(major > 11 || (major === 11 && (minor > 5 || (minor === 5 && patch >= 1))), 'npm >=11.5.1 is required for trusted publishing');
    const result = await resumePublication({
      packages: await recoveryPackages(qualification),
      sdkName: '@cbxss/eoka',
      boundary: await registryBoundary(temporary, env),
      beforePublish: async item => {
        await sourceGate();
        assert.equal(await hash(join(payload, item.filename)), item.sha256);
      },
    });
    process.stdout.write(`${JSON.stringify(result)}\n`);
  } finally {
    await rm(temporary, { recursive: true, force: true });
  }
}

async function smoke() {
  const qualification = await verify();
  const temporary = await mkdtemp(join(tmpdir(), 'eoka-registry-'));
  try {
    const env = await npmEnvironment(temporary);
    for (const key of Object.keys(env)) if (/^(?:EOKA_|ACTIONS_ID_TOKEN_|GH_TOKEN$|GITHUB_TOKEN$)/.test(key)) delete env[key];
    env.HOME = join(temporary, 'home');
    env.XDG_CACHE_HOME = join(temporary, 'cache');
    env.EOKA_RUNTIME_DIR = join(temporary, 'runtime');
    env.EOKA_STATE_DIR = join(temporary, 'state');
    await mkdir(env.HOME);
    const boundary = await registryBoundary(temporary, env);
    await resumePublication({
      packages: await recoveryPackages(qualification),
      sdkName: '@cbxss/eoka',
      boundary: { ...boundary, publish: async () => { throw new Error('Registry smoke cannot publish'); } },
      beforePublish: async item => { throw new Error(`Qualified version is missing from registry: ${item.name}@${item.version}`); },
    });
    const consumer = join(temporary, 'consumer');
    await mkdir(consumer);
    await writeFile(join(consumer, 'package.json'), JSON.stringify({ name: 'eoka-registry-smoke', version: '1.0.0', private: true, type: 'module' }));
    await exec('npm', ['install', `@cbxss/eoka@${qualification.sdkVersion}`, '--ignore-scripts', '--no-audit', '--no-fund', '--registry', registry], { cwd: consumer, env, timeout: 180000, maxBuffer: 8 * 1024 * 1024 });
    await exec('npm', ['audit', 'signatures', '--registry', registry], { cwd: consumer, env, timeout: 180000, maxBuffer: 8 * 1024 * 1024 });
    const { stdout } = await exec(process.execPath, ['--input-type=module', '--eval', `
      import assert from 'node:assert/strict';
      import { createRequire } from 'node:module';
      import { Browser, releaseManifest, detectPlatform } from '@cbxss/eoka';
      assert.equal(releaseManifest.sdkVersion, '0.1.0-beta.1');
      const pin = releaseManifest.server.platforms[detectPlatform()];
      const packagePath = createRequire(import.meta.url).resolve(pin.packageName + '/package.json');
      assert.ok(packagePath.includes('node_modules'), 'Server must come from the registry platform package');
      const browser = await Browser.launch({ browserCacheDir: ${JSON.stringify(join(temporary, 'browser-cache'))}, timeoutMs: 300000 });
      try {
        assert.equal(browser.ownership, 'owned');
        assert.ok(browser.browserVersion.includes(releaseManifest.chrome.buildId));
        const page = await browser.newPage('data:text/html,<title>npm-registry-smoke</title><h1>qualified</h1>');
        assert.equal(await page.title(), 'npm-registry-smoke');
        const png = await page.screenshot({ timeoutMs: 60000 });
        assert.deepEqual(png.subarray(0, 8), Buffer.from([137,80,78,71,13,10,26,10]));
        console.log(JSON.stringify({ status: 'passed', sdkVersion: releaseManifest.sdkVersion, browserVersion: browser.browserVersion, serverPackage: packagePath }));
      } finally { await browser.close(); }
    `], { cwd: consumer, env, timeout: 360000, maxBuffer: 8 * 1024 * 1024 });
    process.stdout.write(stdout);
  } finally {
    await rm(temporary, { recursive: true, force: true });
  }
}

const commands = { gate: sourceGate, download, verify, publish, smoke };
const command = process.argv[2];
assert.ok(process.argv.length === 3 && Object.hasOwn(commands, command), 'Usage: node scripts/publish-release.mjs gate|download|verify|publish|smoke');
await commands[command]();
