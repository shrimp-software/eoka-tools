import assert from 'node:assert/strict';
import { execFile } from 'node:child_process';
import { createHash } from 'node:crypto';
import { chmod, cp, mkdir, mkdtemp, readFile, readdir, rm, symlink, writeFile } from 'node:fs/promises';
import { createServer } from 'node:http';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { promisify } from 'node:util';
import { fileURLToPath } from 'node:url';
import test from 'node:test';
import { chromeArchive } from './fixtures/browser-install-archive.mjs';

const run = promisify(execFile);
const supported = process.platform === 'linux' && process.arch === 'x64';
const nodeRoot = fileURLToPath(new URL('../', import.meta.url));
const loader = fileURLToPath(new URL('./fixtures/browser-install-loader.mjs', import.meta.url));
const runner = fileURLToPath(new URL('./fixtures/browser-managed-runner.mjs', import.meta.url));
const platform = 'linux-x64-gnu';
const buildId = '155.0.8059.39';

async function fixture(t, { delayMs = 0, chunks = false, packaged = true } = {}) {
  const directory = await mkdtemp(join(tmpdir(), 'eoka-bm-'));
  t.after(() => rm(directory, { recursive: true, force: true }));
  const sdk = join(directory, 'sdk');
  const cache = join(directory, 'cache');
  const runtime = join(directory, 'runtime');
  const logPath = join(directory, 'events.jsonl');
  await mkdir(runtime);
  await writeFile(logPath, '');
  await cp(join(nodeRoot, 'dist'), join(sdk, 'dist'), { recursive: true });
  await writeFile(join(sdk, 'package.json'), JSON.stringify({ type: 'module' }));
  await mkdir(join(sdk, 'node_modules'), { recursive: true });
  await symlink(join(nodeRoot, 'node_modules', 'ajv'), join(sdk, 'node_modules', 'ajv'), 'dir');
  await mkdir(join(sdk, 'node_modules', '@puppeteer'));
  await symlink(join(nodeRoot, 'node_modules', '@puppeteer', 'browsers'), join(sdk, 'node_modules', '@puppeteer', 'browsers'), 'dir');
  const manifestPath = join(sdk, 'dist', 'release-manifest.json');
  const manifest = JSON.parse(await readFile(manifestPath, 'utf8'));
  const pin = manifest.server.platforms[platform];
  const packageRoot = join(sdk, 'node_modules', ...pin.packageName.split('/'));
  const serverBytes = Buffer.from(`#!${process.execPath}\nimport ${JSON.stringify(new URL('./fixtures/browser-sidecar.mjs', import.meta.url).href)};\n`);
  pin.sha256 = createHash('sha256').update(serverBytes).digest('hex');
  pin.size = serverBytes.length;
  await writeFile(manifestPath, JSON.stringify(manifest));
  if (packaged) {
    await mkdir(join(packageRoot, 'bin'), { recursive: true });
    await writeFile(join(packageRoot, 'bin', 'eoka-server'), serverBytes);
    await chmod(join(packageRoot, 'bin', 'eoka-server'), 0o700);
    await writeFile(join(packageRoot, 'package.json'), JSON.stringify({ name: pin.packageName, version: pin.packageVersion, os: ['linux'], cpu: ['x64'], libc: ['glibc'] }));
    await writeFile(join(packageRoot, 'release-metadata.json'), JSON.stringify({
      schemaVersion: 1, sdkVersion: manifest.sdkVersion, packageName: pin.packageName, packageVersion: pin.packageVersion,
      platform, target: pin.target, serverVersion: manifest.server.version, protocolVersion: manifest.server.protocolVersion,
      minimumGlibc: pin.minimumGlibc, development: pin.development, binary: { path: pin.executablePath, sha256: pin.sha256, size: pin.size },
    }));
  }
  const { archive, sha256 } = chromeArchive({ executable: `#!/bin/sh\nprintf 'Chrome/${buildId}\\n'\n` });
  let requests = 0;
  const timers = new Set();
  const later = (fn, milliseconds) => {
    const timer = setTimeout(() => { timers.delete(timer); fn(); }, milliseconds);
    timers.add(timer);
  };
  const server = createServer((request, response) => {
    requests++;
    assert.equal(request.url, `/${buildId}/linux64/chrome-linux64.zip`);
    later(() => {
      if (response.destroyed) return;
      response.writeHead(200, { 'content-length': archive.length });
      if (chunks) {
        response.write(archive.subarray(0, Math.floor(archive.length / 2)));
        later(() => response.end(archive.subarray(Math.floor(archive.length / 2))), 1500);
      } else response.end(archive);
    }, delayMs);
  });
  await new Promise(resolve => server.listen(0, '127.0.0.1', resolve));
  t.after(async () => {
    for (const timer of timers) clearTimeout(timer);
    server.closeAllConnections();
    await new Promise(resolve => server.close(resolve));
  });
  const release = { platform, buildId, pin: { platform: 'linux', url: `http://127.0.0.1:${server.address().port}/${buildId}/linux64/chrome-linux64.zip`, sha256, executablePath: 'chrome-linux64/chrome' } };
  const events = async () => (await readFile(logPath, 'utf8')).trim().split('\n').filter(Boolean).map(line => JSON.parse(line));
  let sequence = 0;
  const launch = async ({ env = {}, options = {}, connect = false, abortOnProgress = false } = {}) => {
    const configPath = join(directory, `configuration-${sequence++}.json`);
    await writeFile(configPath, JSON.stringify({
      browserModule: join(sdk, 'dist', 'browser.js'), connect, abortOnProgress,
      options: {
        shutdownTimeoutMs: 3000,
        ...(connect ? { browserWSEndpoint: 'ws://127.0.0.1:9222/devtools/browser/managed-fixture' } : { runtimeDir: runtime }),
        serverEnv: { EOKA_BROWSER_FIXTURE_LOG: logPath, EOKA_BROWSER_FIXTURE_SERVER_VERSION: manifest.server.version, EOKA_BROWSER_FIXTURE_EXECUTE_CHROME: '1', EOKA_BROWSER_CACHE_DIR: cache, ...env },
        ...options,
      },
    }));
    const { stdout } = await run(process.execPath, ['--no-warnings', '--loader', loader, runner, configPath], {
      env: { ...process.env, HTTP_PROXY: undefined, HTTPS_PROXY: undefined, ALL_PROXY: undefined, http_proxy: undefined, https_proxy: undefined, all_proxy: undefined, EOKA_SERVER_BIN: undefined, EOKA_CHROME_PATH: undefined, EOKA_LIFECYCLE_INSTALL_RELEASE: JSON.stringify(release), EOKA_BROWSER_FIXTURE_LOG: logPath },
      timeout: 15000,
    });
    return JSON.parse(stdout);
  };
  return { directory, sdk, cache, runtime, events, launch, requests: () => requests };
}

async function assertNoOwnedStages(f) {
  const stages = await readdir(join(f.cache, '.staging')).catch(error => error.code === 'ENOENT' ? [] : Promise.reject(error));
  assert.deepEqual(stages, []);
  const locksRoot = join(f.cache, '.locks');
  const locks = await readdir(locksRoot).catch(error => error.code === 'ENOENT' ? [] : Promise.reject(error));
  for (const lock of locks) {
    assert.equal(lock.endsWith('.reclaim'), false);
    assert.deepEqual(await readdir(join(locksRoot, lock)), []);
  }
  assert.deepEqual(await readdir(f.runtime), []);
}

test('managed launch resolves the staged server before installing a complete pinned distribution and reuses cache offline', { skip: !supported }, async t => {
  const f = await fixture(t);
  const first = await f.launch();
  assert.equal(first.ok, true, first.message);
  assert.equal(first.ownership, 'owned');
  assert.equal(first.browserVersion, `Chrome/${buildId}`);
  assert.ok(first.progress.length > 0);
  assert.ok(first.progress.every(item => item.buildId === buildId && item.totalBytes > 0));
  const events = await f.events();
  const executable = events.find(event => event.event === 'browser-executable').executablePath;
  assert.ok(executable.startsWith(f.cache));
  assert.equal(await readFile(join(executable, '..', 'resources.pak'), 'utf8'), 'full distribution resource');
  assert.equal(events.find(event => event.method === 'browser.launch').params.sandbox, true);
  const requests = f.requests();
  const second = await f.launch({ options: { download: 'never' } });
  assert.equal(second.ok, true, second.message);
  assert.equal(f.requests(), requests);
  assert.deepEqual(second.progress, []);
  await assertNoOwnedStages(f);
});

test('explicit and environment Chrome overrides bypass the managed installer and cache entirely', { skip: !supported }, async t => {
  for (const override of ['explicit', 'environment']) {
    await t.test(override, async child => {
      const f = await fixture(child);
      const result = await f.launch({
        options: { download: 'never', ...(override === 'explicit' ? { executablePath: process.execPath } : {}) },
        env: { EOKA_BROWSER_FIXTURE_EXECUTE_CHROME: '0', EOKA_CHROME_PATH: override === 'environment' ? process.execPath : '/ignored/chrome' },
      });
      assert.equal(result.ok, true, result.message);
      assert.equal(result.browserVersion, 'Chrome/fixture');
      assert.equal(f.requests(), 0);
      assert.equal((await f.events()).some(event => event.event === 'installer-called'), false);
      await assert.rejects(readdir(f.cache), { code: 'ENOENT' });
      assert.deepEqual(await readdir(f.runtime), []);
    });
  }
});

test('missing staged server fails before installer invocation, download or runtime creation', { skip: !supported }, async t => {
  const f = await fixture(t, { packaged: false });
  const result = await f.launch();
  assert.equal(result.code, 'ServerPackageMissing');
  assert.equal(f.requests(), 0);
  assert.deepEqual(await f.events(), []);
  assert.deepEqual(await readdir(f.runtime), []);
  await assert.rejects(readFile(join(f.cache, 'unused')), { code: 'ENOENT' });
});

test('packaged connect never calls installer despite managed cache and Chrome environment settings', { skip: !supported }, async t => {
  const f = await fixture(t);
  const result = await f.launch({ connect: true, env: { EOKA_CHROME_PATH: '/missing/chrome', EOKA_RUNTIME_DIR: '/not-created' } });
  assert.equal(result.ok, true, result.message);
  assert.equal(result.ownership, 'borrowed');
  assert.equal(f.requests(), 0);
  assert.equal((await f.events()).some(event => event.event === 'installer-called'), false);
  assert.deepEqual(await readdir(f.runtime), []);
});

test('managed version mismatches retain cache and remove owned runtime after teardown', { skip: !supported }, async t => {
  const f = await fixture(t);
  const result = await f.launch({ env: { EOKA_BROWSER_FIXTURE_VERSION: 'Chrome/1.0.0.0' } });
  assert.equal(result.code, 'Protocol');
  assert.match(result.message, /Managed browser version mismatch/);
  const events = await f.events();
  assert.equal(events.some(event => event.event === 'cleanup'), true);
  assert.equal(events.find(event => event.event === 'exit').runtimeExists, true);
  const executable = events.find(event => event.event === 'browser-executable').executablePath;
  assert.ok((await readFile(executable, 'utf8')).includes(buildId));
  await assertNoOwnedStages(f);
});

test('packaged hello version mismatch fails before browser launch and unwinds its runtime', { skip: !supported }, async t => {
  const f = await fixture(t);
  const result = await f.launch({ env: { EOKA_BROWSER_FIXTURE_SERVER_VERSION: '9.9.9' } });
  assert.equal(result.code, 'Protocol');
  assert.match(result.message, /Packaged server version mismatch/);
  assert.equal((await f.events()).some(event => event.method === 'browser.launch'), false);
  await assertNoOwnedStages(f);
});

test('one launch deadline includes the real installer, hello and browser launch', { skip: !supported }, async t => {
  const f = await fixture(t, { delayMs: 1500 });
  const result = await f.launch({ env: { EOKA_BROWSER_FIXTURE_MODE: 'managed-deadline' }, options: { timeoutMs: 4000 } });
  assert.equal(result.code, 'DeadlineExceeded');
  const events = await f.events();
  assert.ok(events.some(event => event.event === 'installer-called'));
  assert.ok(events.some(event => event.method === 'browser.launch'));
  assert.ok(events.some(event => event.event === 'cleanup'));
  await assertNoOwnedStages(f);
});

test('deadline and caller cancellation during installation stop writers before rejecting and do not spawn a sidecar', { skip: !supported }, async t => {
  for (const cancellation of ['deadline', 'abort']) {
    await t.test(cancellation, async child => {
      const f = await fixture(child, { delayMs: cancellation === 'deadline' ? 3000 : 0, chunks: cancellation === 'abort' });
      const result = await f.launch(cancellation === 'deadline' ? { options: { timeoutMs: 1200 } } : { abortOnProgress: true });
      assert.equal(result.code, cancellation === 'deadline' ? 'DeadlineExceeded' : 'Aborted');
      assert.ok(f.requests() > 0);
      assert.equal((await f.events()).some(event => event.event === 'spawn'), false);
      await assertNoOwnedStages(f);
      const published = (await readdir(f.cache)).filter(name => !name.startsWith('.'));
      assert.deepEqual(published, []);
    });
  }
});
