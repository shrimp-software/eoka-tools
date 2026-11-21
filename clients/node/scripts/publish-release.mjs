import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { execFile } from 'node:child_process';
import { mkdir, mkdtemp, readFile, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { promisify } from 'node:util';

const exec = promisify(execFile);
const registry = 'https://registry.npmjs.org/';
const sdkName = '@cbxss/eoka';
const [command, directory] = process.argv.slice(2);
assert.ok(['publish', 'smoke'].includes(command) && directory, 'Usage: node scripts/publish-release.mjs publish|smoke PACKAGES_DIR');
const packagesDir = resolve(directory);
const { manifest, packages } = JSON.parse(await readFile(join(packagesDir, 'packages.json'), 'utf8'));
assert.equal(packages.length, 6, 'The SDK and all five platform packages are required');
const ordered = [...packages.filter(item => item.name !== sdkName), packages.find(item => item.name === sdkName)];
const integrity = async item => `sha512-${createHash('sha512').update(await readFile(join(packagesDir, item.filename))).digest('base64')}`;

async function packument(name) {
  const response = await fetch(`${registry}${encodeURIComponent(name)}`, { headers: { Accept: 'application/json', 'Cache-Control': 'no-cache' }, signal: AbortSignal.timeout(120000) });
  if (response.status === 404) return null;
  assert.equal(response.status, 200, `Registry read failed for ${name}: ${response.status}`);
  return response.json();
}

async function cleanNpm(scratch) {
  const env = { ...process.env, NPM_CONFIG_USERCONFIG: join(scratch, 'npmrc'), NPM_CONFIG_GLOBALCONFIG: join(scratch, 'global-npmrc'), NPM_CONFIG_REGISTRY: registry, NPM_CONFIG_CACHE: join(scratch, 'npm-cache') };
  for (const key of Object.keys(env)) if (/^(?:npm_config_(?!userconfig$|globalconfig$|registry$|cache$)|NODE_AUTH_TOKEN$|NPM_TOKEN$)/i.test(key)) delete env[key];
  for (const config of [env.NPM_CONFIG_USERCONFIG, env.NPM_CONFIG_GLOBALCONFIG]) await writeFile(config, '', { mode: 0o600 });
  return env;
}

async function publish(env, scratch) {
  const { stdout } = await exec('npm', ['--version'], { env });
  const [major, minor, patch] = stdout.trim().split('.').map(Number);
  assert.ok(major > 11 || (major === 11 && (minor > 5 || (minor === 5 && patch >= 1))), 'npm >=11.5.1 is required for trusted publishing');
  for (const item of ordered) {
    const existing = (await packument(item.name))?.versions?.[item.version];
    if (existing) {
      assert.equal(existing.dist?.integrity, await integrity(item), `${item.name}@${item.version} is already on npm with different contents`);
      console.log(`${item.name}@${item.version} is already published`);
      continue;
    }
    const result = await exec('npm', ['publish', join(packagesDir, item.filename), '--ignore-scripts', '--access', 'public', '--tag', 'latest', '--provenance'], { cwd: scratch, env, timeout: 180000, maxBuffer: 8 * 1024 * 1024 });
    process.stdout.write(result.stdout);
    process.stderr.write(result.stderr);
  }
}

// npm acknowledges a publish before the packument and tarball are served.
async function awaitRegistry({ timeoutMs = 900000, intervalMs = 15000 } = {}) {
  const deadline = Date.now() + timeoutMs;
  for (const item of ordered) {
    const expected = await integrity(item);
    for (;;) {
      const metadata = await packument(item.name);
      const dist = metadata?.['dist-tags']?.latest === item.version ? metadata.versions?.[item.version]?.dist : undefined;
      if (dist) {
        assert.equal(dist.integrity, expected, `Registry tarball differs from the qualified one: ${item.name}`);
        if ((await fetch(dist.tarball, { method: 'HEAD', signal: AbortSignal.timeout(120000) })).ok) break;
      }
      assert.ok(Date.now() < deadline, `Registry did not serve ${item.name}@${item.version} within ${timeoutMs / 1000}s`);
      await new Promise(resolve => setTimeout(resolve, intervalMs));
    }
  }
}

async function smoke(env, scratch) {
  await awaitRegistry();
  for (const key of Object.keys(env)) if (/^(?:EOKA_|ACTIONS_ID_TOKEN_|GH_TOKEN$|GITHUB_TOKEN$)/.test(key)) delete env[key];
  Object.assign(env, { HOME: join(scratch, 'home'), XDG_CACHE_HOME: join(scratch, 'cache'), EOKA_RUNTIME_DIR: scratch, EOKA_STATE_DIR: join(scratch, 'state') });
  await mkdir(env.HOME);
  const consumer = join(scratch, 'consumer');
  await mkdir(consumer);
  await writeFile(join(consumer, 'package.json'), JSON.stringify({ name: 'eoka-registry-smoke', version: '1.0.0', private: true, type: 'module' }));
  const run = (file, args, timeout = 180000) => exec(file, args, { cwd: consumer, env, timeout, maxBuffer: 8 * 1024 * 1024 });
  await run('npm', ['install', `${sdkName}@${manifest.sdkVersion}`, '--ignore-scripts', '--no-audit', '--no-fund']);
  await run('npm', ['audit', 'signatures']);
  const { stdout } = await run(process.execPath, ['--input-type=module', '--eval', `
    import assert from 'node:assert/strict';
    import { createRequire } from 'node:module';
    import { Browser, releaseManifest, detectPlatform } from '@cbxss/eoka';
    assert.equal(releaseManifest.sdkVersion, ${JSON.stringify(manifest.sdkVersion)});
    const pin = releaseManifest.server.platforms[detectPlatform()];
    assert.ok(createRequire(import.meta.url).resolve(pin.packageName + '/package.json').includes('node_modules'));
    const browser = await Browser.launch({ browserCacheDir: ${JSON.stringify(join(scratch, 'browser-cache'))}, timeoutMs: 300000 });
    try {
      assert.ok(browser.browserVersion.includes(releaseManifest.chrome.buildId));
      const page = await browser.newPage('data:text/html,<title>npm-registry-smoke</title>');
      assert.equal(await page.title(), 'npm-registry-smoke');
      console.log(JSON.stringify({ status: 'passed', sdkVersion: releaseManifest.sdkVersion, browserVersion: browser.browserVersion }));
    } finally { await browser.close(); }
  `], 360000);
  process.stdout.write(stdout);
}

const scratch = await mkdtemp(join(tmpdir(), 'eoka-release-'));
try {
  const env = await cleanNpm(scratch);
  await (command === 'publish' ? publish : smoke)(env, scratch);
} finally {
  await rm(scratch, { recursive: true, force: true });
}
