import assert from 'node:assert/strict';
import { fork } from 'node:child_process';
import { createHash } from 'node:crypto';
import { createServer } from 'node:http';
import { chmod, lstat, mkdir, mkdtemp, readFile, readdir, rm, symlink, writeFile } from 'node:fs/promises';
import { hostname, tmpdir } from 'node:os';
import path from 'node:path';
import test from 'node:test';
import { setTimeout as delay } from 'node:timers/promises';
import { browserCacheDirectory, createBrowserInstaller } from '../dist/browser-install-engine.js';
import { installBrowser } from '../dist/browser-install.js';
import { sanitizeInstallationDiagnostic } from '../dist/browser-install-diagnostics.js';
import { chromeArchive } from './fixtures/browser-install-archive.mjs';

const buildId = '155.0.8059.39';
const supported = process.platform === 'linux' && process.arch === 'x64';

async function fixture(t, { data = chromeArchive(), stall = false } = {}) {
  const cacheDir = await mkdtemp(path.join(tmpdir(), 'eoka install test '));
  const requests = [];
  const server = createServer((request, response) => {
    requests.push(request.url);
    if (server.stall) {
      response.writeHead(200, { 'Content-Length': data.archive.length });
      response.write(data.archive.subarray(0, 10));
    } else {
      response.writeHead(200, { 'Content-Length': data.archive.length });
      response.end(data.archive);
    }
  });
  server.stall = stall;
  await new Promise(resolve => server.listen(0, '127.0.0.1', resolve));
  t.after(async () => {
    server.closeAllConnections();
    await new Promise(resolve => server.close(resolve));
    await chmod(cacheDir, 0o700);
    for (const [directory] of await snapshot(cacheDir)) {
      if ((await lstat(directory)).isDirectory()) await chmod(directory, 0o700);
    }
    await rm(cacheDir, { recursive: true, force: true });
  });
  const release = { platform: 'linux-x64-gnu', buildId, pin: { url: `http://127.0.0.1:${server.address().port}/chrome-for-testing-public/${buildId}/linux64/chrome-linux64.zip`, sha256: data.sha256, platform: 'linux', executablePath: 'chrome-linux64/chrome' } };
  const install = createBrowserInstaller(release);
  return { cacheDir, release, install, requests, server };
}

async function processInstall(release, options) {
  const child = fork(new URL('./fixtures/browser-install-process.mjs', import.meta.url), [JSON.stringify({ release, options })], { stdio: ['ignore', 'ignore', 'pipe', 'ipc'], execArgv: [] });
  let message;
  let stderr = '';
  child.on('message', value => { message = value; });
  child.stderr.on('data', chunk => { stderr += chunk; });
  await new Promise((resolve, reject) => { child.once('error', reject); child.once('close', resolve); });
  assert.ok(message, stderr);
  if (message.error) throw Object.assign(new Error(message.error.message), { code: message.error.code });
  return message.result;
}

async function snapshot(root) {
  const output = [];
  for (const name of (await readdir(root)).sort()) {
    const absolute = path.join(root, name);
    const info = await lstat(absolute);
    output.push([absolute, info.mtimeMs, info.mode, info.isFile() ? createHash('sha256').update(await readFile(absolute)).digest('hex') : null]);
    if (info.isDirectory()) output.push(...await snapshot(absolute));
  }
  return output;
}

async function assertUnpublished(cacheDir) {
  assert.deepEqual((await readdir(cacheDir)).filter(name => !name.startsWith('.')), []);
  assert.deepEqual(await readdir(path.join(cacheDir, '.staging')).catch(error => error.code === 'ENOENT' ? [] : Promise.reject(error)), []);
  const locks = await readdir(path.join(cacheDir, '.locks')).catch(error => error.code === 'ENOENT' ? [] : Promise.reject(error));
  for (const lock of locks) assert.deepEqual(await readdir(path.join(cacheDir, '.locks', lock)), []);
}

test('cache precedence is explicit, environment, then per-user default', () => {
  assert.equal(browserCacheDirectory({ browserCacheDir: './first', env: { EOKA_BROWSER_CACHE_DIR: './second' } }), path.resolve('./first'));
  assert.equal(browserCacheDirectory({ env: { EOKA_BROWSER_CACHE_DIR: './second' } }), path.resolve('./second'));
  if (process.platform === 'linux') assert.equal(browserCacheDirectory({ env: { XDG_CACHE_HOME: '/custom cache' } }), '/custom cache/eoka');
  assert.throws(() => browserCacheDirectory({ browserCacheDir: '' }), { code: 'InstallationFailed' });
});

test('full pinned archive installs, reports progress, and verified read-only offline cache performs no writes', { skip: !supported }, async t => {
  const f = await fixture(t);
  const progress = [];
  const first = await f.install({ browserCacheDir: f.cacheDir, onProgress: update => progress.push(update) });
  assert.equal(first.source, 'download');
  assert.equal(await readFile(path.join(path.dirname(first.executablePath), 'resources.pak'), 'utf8'), 'full distribution resource');
  assert.ok(progress.some(update => update.downloadedBytes === update.totalBytes && update.totalBytes > 0 && update.buildId === buildId));
  const directories = [];
  for (const [name] of await snapshot(f.cacheDir)) if ((await lstat(name)).isDirectory()) directories.push([name]);
  for (const [directory] of directories.reverse()) await chmod(directory, 0o555);
  await chmod(f.cacheDir, 0o555);
  const before = await snapshot(f.cacheDir);
  const second = await f.install({ browserCacheDir: f.cacheDir, download: 'never' });
  assert.equal(second.source, 'cache');
  assert.equal(second.executablePath, first.executablePath);
  assert.deepEqual(await snapshot(f.cacheDir), before);
  assert.equal(f.requests.length, 1);
});

test('empty runtime extensions directory does not invalidate the pinned cache', { skip: !supported }, async t => {
  const f = await fixture(t);
  const first = await f.install({ browserCacheDir: f.cacheDir });
  // Chrome creates an empty `extensions/` directory next to its executable on
  // startup (component-extension staging). The next launch must still verify.
  const runtime = path.join(path.dirname(first.executablePath), 'extensions');
  await mkdir(runtime);
  const second = await f.install({ browserCacheDir: f.cacheDir, download: 'never' });
  assert.equal(second.source, 'cache');
  assert.equal(second.executablePath, first.executablePath);
  assert.equal(f.requests.length, 1);

  // Anything actually written into that runtime directory is still tampering.
  await writeFile(path.join(runtime, 'component.crx'), 'not pinned');
  await assert.rejects(f.install({ browserCacheDir: f.cacheDir, download: 'never' }), { code: 'IntegrityMismatch' });
});

test('empty extensions directory elsewhere in the tree still invalidates the cache', { skip: !supported }, async t => {
  const f = await fixture(t);
  const first = await f.install({ browserCacheDir: f.cacheDir });
  // Only the directory beside the executable is runtime-created.
  await mkdir(path.join(path.dirname(path.dirname(first.executablePath)), 'extensions'));
  await assert.rejects(f.install({ browserCacheDir: f.cacheDir, download: 'never' }), { code: 'IntegrityMismatch' });
});

test('offline cache miss makes no cache directories and performs no network', { skip: !supported }, async t => {
  const f = await fixture(t);
  await assert.rejects(f.install({ browserCacheDir: f.cacheDir, download: 'never' }), { code: 'BrowserNotInstalled' });
  assert.equal(f.requests.length, 0);
  assert.deepEqual(await readdir(f.cacheDir), []);
});

test('wrong archive hash is rejected without publication', { skip: !supported }, async t => {
  const f = await fixture(t);
  f.release.pin.sha256 = '0'.repeat(64);
  await assert.rejects(createBrowserInstaller(f.release)({ browserCacheDir: f.cacheDir }), { code: 'IntegrityMismatch' });
  await assertUnpublished(f.cacheDir);
});

test('checksum-valid damaged ZIP still cannot publish', { skip: !supported }, async t => {
  const archive = Buffer.from('not a zip archive');
  const f = await fixture(t, { data: { archive, sha256: createHash('sha256').update(archive).digest('hex') } });
  await assert.rejects(f.install({ browserCacheDir: f.cacheDir }), { code: 'InstallationFailed' });
  await assertUnpublished(f.cacheDir);
});

for (const corruption of ['resource', 'extra', 'marker', 'executable', 'link']) {
  test(`cache detects ${corruption} corruption without redownload or deleting user resources`, { skip: !supported }, async t => {
    const f = await fixture(t);
    const installed = await f.install({ browserCacheDir: f.cacheDir });
    const distribution = path.dirname(installed.executablePath);
    const root = path.join(f.cacheDir, (await readdir(f.cacheDir)).find(name => !name.startsWith('.')));
    if (corruption === 'resource') await writeFile(path.join(distribution, 'resources.pak'), 'corrupted resource');
    if (corruption === 'extra') await writeFile(path.join(distribution, 'unexpected.pak'), 'unexpected resource');
    if (corruption === 'marker') await writeFile(path.join(root, '.eoka-installation.json'), '{}');
    if (corruption === 'executable') await chmod(installed.executablePath, 0o644);
    if (corruption === 'link') { await rm(path.join(distribution, 'resources.pak')); await symlink('/etc/passwd', path.join(distribution, 'resources.pak')); }
    const before = await snapshot(f.cacheDir);
    await assert.rejects(f.install({ browserCacheDir: f.cacheDir }), { code: 'IntegrityMismatch' });
    assert.equal(f.requests.length, 1);
    assert.deepEqual(await snapshot(f.cacheDir), before);
  });
}

test('two independent processes converge on one complete distribution', { skip: !supported }, async t => {
  const f = await fixture(t);
  const results = await Promise.all([processInstall(f.release, { browserCacheDir: f.cacheDir }), processInstall(f.release, { browserCacheDir: f.cacheDir })]);
  assert.equal(results[0].executablePath, results[1].executablePath);
  assert.deepEqual(results.map(result => result.source).sort(), ['cache', 'download']);
  assert.equal(f.requests.length, 1);
  assert.equal((await f.install({ browserCacheDir: f.cacheDir, download: 'never' })).source, 'cache');
});

for (const control of ['abort', 'timeout']) {
  test(`${control} terminates stalled download and leaves no complete or owned staging tree`, { skip: !supported }, async t => {
    const f = await fixture(t, { stall: true });
    const controller = new AbortController();
    const pending = f.install({ browserCacheDir: f.cacheDir, signal: controller.signal, timeoutMs: control === 'timeout' ? 1500 : 10_000 });
    if (control === 'abort') {
      while (!f.requests.length) await delay(20);
      controller.abort();
    }
    await assert.rejects(pending, { code: control === 'abort' ? 'InstallationAborted' : 'InstallationTimeout' });
    await assertUnpublished(f.cacheDir);
    await delay(100);
    await assertUnpublished(f.cacheDir);
  });
}

test('pre-aborted signal touches neither cache nor network', { skip: !supported }, async t => {
  const f = await fixture(t);
  await assert.rejects(f.install({ browserCacheDir: f.cacheDir, signal: AbortSignal.abort() }), { code: 'InstallationAborted' });
  assert.equal(f.requests.length, 0);
  assert.deepEqual(await readdir(f.cacheDir), []);
});

test('dead owner lock is recovered while active owner lock remains untouched on timeout', { skip: !supported }, async t => {
  const f = await fixture(t);
  const key = `chrome-${f.release.platform}-${buildId}-${f.release.pin.sha256}`;
  const lock = path.join(f.cacheDir, '.locks', key);
  await mkdir(lock, { recursive: true });
  const owner = { token: 'dead', pid: 2147483647, host: hostname(), processStart: null, workerPid: null, workerStart: null, ticket: 1 };
  await writeFile(path.join(lock, 'dead.json'), JSON.stringify(owner));
  assert.equal((await f.install({ browserCacheDir: f.cacheDir })).source, 'download');
  await rm(path.join(f.cacheDir, key), { recursive: true });
  owner.pid = process.pid;
  owner.token = 'borrowed';
  await writeFile(path.join(lock, 'borrowed.json'), JSON.stringify(owner));
  await assert.rejects(f.install({ browserCacheDir: f.cacheDir, timeoutMs: 500 }), { code: 'InstallationTimeout' });
  assert.equal(JSON.parse(await readFile(path.join(lock, 'borrowed.json'), 'utf8')).token, 'borrowed');
  assert.equal(f.requests.length, 1);
});

test('orphaned ownerless lock directory is immediately reusable', { skip: !supported }, async t => {
  const f = await fixture(t);
  const key = `chrome-${f.release.platform}-${buildId}-${f.release.pin.sha256}`;
  const lock = path.join(f.cacheDir, '.locks', key);
  await mkdir(lock, { recursive: true });
  assert.equal((await f.install({ browserCacheDir: f.cacheDir })).source, 'download');
});

test('progress callback failure cancels real writer before cleanup', { skip: !supported }, async t => {
  const f = await fixture(t);
  await assert.rejects(f.install({ browserCacheDir: f.cacheDir, onProgress() { throw new Error('callback failure'); } }), { code: 'InstallationFailed' });
  await assertUnpublished(f.cacheDir);
});

test('insecure TLS environment is rejected before download without mutating parent env', { skip: !supported }, async t => {
  const f = await fixture(t);
  const before = process.env.NODE_TLS_REJECT_UNAUTHORIZED;
  await assert.rejects(f.install({ browserCacheDir: f.cacheDir, env: { ...process.env, NODE_TLS_REJECT_UNAUTHORIZED: '0' } }), /require TLS certificate verification/);
  assert.equal(process.env.NODE_TLS_REJECT_UNAUTHORIZED, before);
  assert.equal(f.requests.length, 0);
  await assertUnpublished(f.cacheDir);
});

test('public managed installer has no arbitrary release overrides', { skip: !supported }, async t => {
  const f = await fixture(t);
  await assert.rejects(installBrowser({ browserCacheDir: f.cacheDir, download: 'never', release: f.release }), { code: 'BrowserNotInstalled' });
  assert.equal(f.requests.length, 0);
});

test('checksum-valid archive with escaping link is rejected before publication', { skip: !supported }, async t => {
  const f = await fixture(t, { data: chromeArchive({ unsafeLink: true }) });
  await assert.rejects(f.install({ browserCacheDir: f.cacheDir }), { code: 'IntegrityMismatch' });
  await assertUnpublished(f.cacheDir);
});

test('proxy is honored in child environment without parent mutation or credential diagnostics', { skip: !supported }, async t => {
  const f = await fixture(t);
  const proxyRequests = [];
  const proxy = createServer((request, response) => {
    proxyRequests.push({ url: request.url, authorization: request.headers['proxy-authorization'] });
    response.writeHead(407);
    response.end('proxy authentication required');
  });
  await new Promise(resolve => proxy.listen(0, '127.0.0.1', resolve));
  t.after(async () => { proxy.closeAllConnections(); await new Promise(resolve => proxy.close(resolve)); });
  const proxyUrl = `http://private-user:private-password@127.0.0.1:${proxy.address().port}`;
  const env = { ...process.env, HTTP_PROXY: proxyUrl, http_proxy: proxyUrl, HTTPS_PROXY: proxyUrl, https_proxy: proxyUrl, ALL_PROXY: proxyUrl, all_proxy: proxyUrl, NO_PROXY: '', no_proxy: '' };
  const previous = process.env.HTTP_PROXY;
  await assert.rejects(f.install({ browserCacheDir: f.cacheDir, env }), error => {
    assert.equal(error.code, 'InstallationFailed');
    assert.doesNotMatch(`${error.message} ${error.stderr}`, /private-user|private-password/);
    return true;
  });
  assert.equal(process.env.HTTP_PROXY, previous);
  assert.equal(proxyRequests.length, 1);
  assert.equal(proxyRequests[0].url, f.release.pin.url);
  assert.equal(f.requests.length, 0);
  await assertUnpublished(f.cacheDir);
});

test('parent process death cancels owned worker and stale recovery never treats interrupted tree as complete', { skip: !supported, timeout: 15_000 }, async t => {
  const f = await fixture(t, { stall: true });
  const child = fork(new URL('./fixtures/browser-install-process.mjs', import.meta.url), [JSON.stringify({ release: f.release, options: { browserCacheDir: f.cacheDir, timeoutMs: 10_000 } })], { stdio: ['ignore', 'ignore', 'ignore', 'ipc'], execArgv: [] });
  t.after(() => { if (child.exitCode === null && child.signalCode === null) child.kill('SIGKILL'); });
  while (!f.requests.length) await delay(20);
  const closed = new Promise(resolve => child.once('close', resolve));
  child.kill('SIGKILL');
  await closed;
  assert.deepEqual((await readdir(f.cacheDir)).filter(name => !name.startsWith('.')), []);
  f.server.stall = false;
  const result = await f.install({ browserCacheDir: f.cacheDir, timeoutMs: 5000 });
  assert.equal(result.source, 'download');
  assert.equal((await f.install({ browserCacheDir: f.cacheDir, download: 'never' })).source, 'cache');
  assert.equal(f.requests.length, 2);
});

test('proxy credentials are removed from structured installer diagnostics', () => {
  const proxy = 'http://private-user:private-password@proxy.invalid:8080';
  const credential = Buffer.from('private-user:private-password').toString('base64');
  const diagnostic = sanitizeInstallationDiagnostic(`Proxy ${proxy} rejected private-user and private-password; Authorization: Basic ${credential}`, { HTTPS_PROXY: proxy });
  assert.doesNotMatch(diagnostic, /private-user|private-password/);
  assert.ok(!diagnostic.includes(credential));
  assert.match(diagnostic, /Proxy/);
  assert.match(diagnostic, /rejected/);
});
