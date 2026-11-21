import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { execFile } from 'node:child_process';
import { access, mkdir, mkdtemp, readFile, realpath, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { delimiter, dirname, join, resolve } from 'node:path';
import { promisify } from 'node:util';

const exec = promisify(execFile);
const platforms = ['linux-x64-gnu', 'linux-arm64-gnu', 'darwin-x64', 'darwin-arm64', 'win32-x64'];
const repository = 'git+https://github.com/shrimp-software/eoka-tools.git';
const digest = bytes => createHash('sha256').update(bytes).digest('hex');
const json = async path => JSON.parse(await readFile(path, 'utf8'));
const save = (path, value) => writeFile(path, `${JSON.stringify(value, null, 2)}\n`);

async function npm(args, cwd) {
  const options = { cwd, maxBuffer: 16 * 1024 * 1024, timeout: 300000, env: { ...process.env, npm_config_update_notifier: 'false' } };
  if (process.platform !== 'win32') return exec('npm', args, options);
  for (const directory of (process.env.PATH ?? '').split(delimiter)) {
    try {
      const cli = join(dirname(await realpath(join(directory, 'npm.cmd'))), 'node_modules', 'npm', 'bin', 'npm-cli.js');
      await access(cli);
      return exec(process.execPath, [cli, ...args], options);
    } catch (error) {
      if (!['ENOENT', 'ENOTDIR'].includes(error.code)) throw error;
    }
  }
  throw new Error('Cannot locate the npm JavaScript CLI on PATH');
}

async function pack(stage, output) {
  await mkdir(output, { recursive: true });
  const manifest = await json(join(stage, 'sdk', 'release-manifest.json'));
  const directories = [join(stage, 'sdk'), ...platforms.filter(id => manifest.server.platforms[id].sha256 !== null).map(id => join(stage, 'platforms', id))];
  const packages = [];
  for (const directory of directories) {
    const metadata = await json(join(directory, 'package.json'));
    assert.equal(metadata.repository.url, repository);
    assert.ok(!Object.keys(metadata.scripts ?? {}).some(key => /^(?:pre|post)?(?:install|publish|prepare)$|^prepublishOnly$/.test(key)), `Lifecycle scripts are forbidden: ${metadata.name}`);
    const { stdout } = await npm(['pack', '--json', '--ignore-scripts', '--pack-destination', output], directory);
    const [packed] = JSON.parse(stdout);
    assert.equal(packed.name, metadata.name);
    assert.equal(packed.version, manifest.sdkVersion);
    packages.push({ filename: packed.filename, name: metadata.name, version: metadata.version, sha256: digest(await readFile(join(output, packed.filename))) });
  }
  await save(join(output, 'packages.json'), { manifest, packages });
}

async function consumer(config) {
  const assert = (await import('node:assert/strict')).default;
  const { createHash } = await import('node:crypto');
  const { execFile } = await import('node:child_process');
  const { mkdir, mkdtemp, readFile, readdir, writeFile } = await import('node:fs/promises');
  const { createRequire } = await import('node:module');
  const { dirname, join } = await import('node:path');
  const { setTimeout: delay } = await import('node:timers/promises');
  const { promisify } = await import('node:util');
  const exec = promisify(execFile);
  const { Browser, detectPlatform, installBrowser, releaseManifest } = await import('@cbxss/eoka');
  const pin = releaseManifest.server.platforms[detectPlatform()];
  const binary = join(dirname(createRequire(import.meta.url).resolve(`${pin.packageName}/package.json`)), pin.executablePath);
  const bytes = await readFile(binary);
  assert.equal(createHash('sha256').update(bytes).digest('hex'), pin.sha256);
  assert.equal(bytes.length, pin.size);
  const corrupt = Buffer.from(bytes);
  corrupt[corrupt.length - 1] ^= 1;
  await writeFile(binary, corrupt);
  try {
    await assert.rejects(Browser.launch({ browserCacheDir: config.cache, download: 'never', timeoutMs: 30000 }), error => error.code === 'IntegrityMismatch');
  } finally {
    await writeFile(binary, bytes);
  }
  await mkdir(config.cache, { recursive: true });
  const cache = await mkdtemp(join(config.cache, 'q-'));
  const chrome = await installBrowser({ browserCacheDir: cache, timeoutMs: 300000 });
  assert.equal(chrome.buildId, releaseManifest.chrome.buildId);
  assert.equal((await installBrowser({ browserCacheDir: cache, download: 'never' })).source, 'cache');
  const runtime = join(config.scratch, 'r');
  await mkdir(runtime);
  const browser = await Browser.launch({ browserCacheDir: cache, download: 'never', runtimeDir: runtime, viewport: { width: 800, height: 600, deviceScaleFactor: 1 }, timeoutMs: 60000 });
  try {
    assert.equal(browser.hello.serverVersion, releaseManifest.server.version);
    assert.equal(`${browser.hello.protocol.major}.${browser.hello.protocol.minor}`, releaseManifest.server.protocolVersion);
    const page = await browser.newPage(`data:text/html,${encodeURIComponent('<!doctype html><title>Qualification</title><input id="field"><div style="width:80px;height:80px;background:orange;animation:m 1s alternate infinite"></div><style>@keyframes m{to{transform:translateX(600px)}}</style>')}`);
    assert.equal((await page.fill('#field', 'packed native qualification')).outcome, 'verified_complete');
    assert.equal(await page.title(), 'Qualification');
    const png = await page.screenshot({ format: 'png' });
    assert.deepEqual([...png.subarray(0, 8)], [137, 80, 78, 71, 13, 10, 26, 10]);
    const path = join(config.scratch, 'recording.mp4');
    const recording = await page.record({ path, fps: 30 });
    await delay(2000);
    const artifact = await recording.stop({ timeoutMs: 60000 });
    assert.equal(artifact.mimeType, 'video/mp4');
    const { stdout } = await exec('ffprobe', ['-v', 'error', '-select_streams', 'v:0', '-show_packets', '-of', 'json', path], { maxBuffer: 32 * 1024 * 1024 });
    assert.ok(JSON.parse(stdout).packets.length >= 2, 'Recording has too few samples');
    await exec('ffmpeg', ['-nostdin', '-v', 'error', '-xerror', '-i', path, '-f', 'null', '-'], { maxBuffer: 32 * 1024 * 1024 });
  } finally {
    await browser.close();
  }
  assert.deepEqual(await readdir(runtime), [], 'Owned runtime was not removed');
  console.log(JSON.stringify({ status: 'passed', platform: detectPlatform(), browserVersion: chrome.buildId }));
}

const options = {};
for (let index = 2; index < process.argv.length; index++) {
  const arg = process.argv[index];
  if (arg === '--pack-only') options.packOnly = true;
  else if (['--stage', '--cache', '--output', '--packages'].includes(arg)) {
    const value = process.argv[++index];
    if (!value || value.startsWith('--')) throw new Error(`Missing value for ${arg}`);
    options[arg.slice(2)] = resolve(value);
  } else throw new Error(`Unknown argument: ${arg}`);
}

if (options.packOnly) {
  assert.ok(options.stage && options.output, 'Usage: node qualify-release.mjs --pack-only --stage DIR --output DIR');
  await pack(options.stage, options.output);
} else {
  assert.ok(options.packages && options.cache, 'Usage: node qualify-release.mjs --packages DIR --cache DIR');
  const { manifest, packages } = await json(join(options.packages, 'packages.json'));
  for (const entry of packages) assert.equal(digest(await readFile(join(options.packages, entry.filename))), entry.sha256);
  const platform = process.platform === 'linux' ? `linux-${process.arch}-gnu` : `${process.platform}-${process.arch}`;
  const pin = manifest.server.platforms[platform];
  assert.ok(pin, `Unsupported qualification host ${platform}`);
  if (process.getuid) assert.notEqual(process.getuid(), 0, 'Sandboxed qualification must run as a non-root user');
  const sdk = packages.find(entry => entry.name === '@cbxss/eoka');
  const native = packages.find(entry => entry.name === pin.packageName);
  assert.ok(sdk && native);
  const base = process.platform === 'win32' ? resolve(process.env.SystemDrive || 'C:', '/q') : tmpdir();
  await mkdir(base, { recursive: true });
  const scratch = await realpath(await mkdtemp(join(base, 'eq-')));
  const env = { ...process.env, CARGO_HOME: join(scratch, 'no-cargo') };
  for (const key of ['EOKA_SERVER_BIN', 'EOKA_CHROME_PATH', 'EOKA_RUNTIME_DIR']) delete env[key];
  const pathKey = Object.keys(env).find(key => key.toLowerCase() === 'path') ?? 'PATH';
  env[pathKey] = (env[pathKey] ?? '').split(delimiter).filter(directory => !/(?:cargo|rustup|rust-toolchain|[\\/]target[\\/])/i.test(directory)).join(delimiter);
  try {
    const app = join(scratch, 'a');
    await mkdir(app);
    await save(join(app, 'package.json'), { private: true, type: 'module', dependencies: { '@cbxss/eoka': `file:${join(options.packages, sdk.filename)}`, [pin.packageName]: `file:${join(options.packages, native.filename)}` } });
    await npm(['install', '--ignore-scripts', '--omit=optional', '--no-audit', '--no-fund'], app);
    await writeFile(join(app, 'qualify.mjs'), `await (${consumer.toString()})(${JSON.stringify({ scratch, cache: options.cache })});\n`);
    const result = await exec(process.execPath, [join(app, 'qualify.mjs')], { cwd: app, env, maxBuffer: 32 * 1024 * 1024, timeout: 900000 }).catch(error => error);
    process.stdout.write(result.stdout ?? '');
    process.stderr.write(result.stderr ?? '');
    if (result instanceof Error) throw result;
  } finally {
    await rm(scratch, { recursive: true, force: true });
  }
}
