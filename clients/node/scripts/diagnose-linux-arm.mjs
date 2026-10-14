import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { spawn } from 'node:child_process';
import { once } from 'node:events';
import { chmod, copyFile, cp, mkdir, mkdtemp, open, readFile, readdir, rm, stat, symlink, writeFile } from 'node:fs/promises';
import { createRequire } from 'node:module';
import { dirname, join, resolve } from 'node:path';
import { pathToFileURL } from 'node:url';

const packageDirectory = resolve(process.argv[2] ?? 'packages');
const output = resolve(process.argv[3] ?? 'evidence/diagnose-linux-arm');
const digest = bytes => createHash('sha256').update(bytes).digest('hex');
const save = (name, value) => writeFile(join(output, name), `${JSON.stringify(value, null, 2)}\n`);
const failure = error => ({ name: error.name, message: error.message, stack: error.stack, code: error.code, stderr: error.stderr });
const quote = value => `'${value.replaceAll("'", "'\\''")}'`;
await mkdir(output, { recursive: true });
assert.equal(process.platform, 'linux');
assert.equal(process.arch, 'arm64');
assert.notEqual(process.getuid(), 0);

async function command(name, executable, args, options = {}) {
  const stdout = await open(join(output, `${name}.stdout`), 'w');
  const stderr = await open(join(output, `${name}.stderr`), 'w');
  const started = Date.now();
  let child;
  let timer;
  let timedOut = false;
  try {
    await save(`${name}.command.json`, { executable, args });
    child = spawn(executable, args, { cwd: options.cwd, env: options.env ?? process.env, detached: true, stdio: ['ignore', stdout.fd, stderr.fd] });
    timer = setTimeout(() => {
      timedOut = true;
      try { process.kill(-child.pid, 'SIGKILL'); } catch (error) { if (error.code !== 'ESRCH') throw error; }
    }, options.timeoutMs ?? 30000);
    const [code, signal] = await once(child, 'close');
    const result = { code, signal, timedOut, elapsedMs: Date.now() - started };
    await save(`${name}.result.json`, result);
    return result;
  } catch (error) {
    const result = { error: failure(error), timedOut, elapsedMs: Date.now() - started };
    await save(`${name}.result.json`, result);
    return result;
  } finally {
    clearTimeout(timer);
    await stdout.close();
    await stderr.close();
  }
}

const summary = { schemaVersion: 1, diagnosticOnly: true, qualificationStatus: 'not_run', recordingReadiness: 'blocked', recordingBlocker: 'native-recording-duplicate-terminal-dts', sandboxRequested: true, sourceSha: process.env.EOKA_SOURCE_SHA ?? null, candidateRunId: process.env.CANDIDATE_RUN_ID ?? null, probes: {} };
let scratch;
try {
  await command('host', 'bash', ['-c', 'uname -a; id; cat /etc/os-release; getconf GNU_LIBC_VERSION; getconf PAGESIZE; ulimit -a; df -h /tmp /dev/shm; findmnt -T /tmp; cat /proc/self/status; cat /proc/self/attr/current; cat /proc/sys/kernel/apparmor_restrict_unprivileged_userns /proc/sys/kernel/unprivileged_userns_clone /proc/sys/user/max_user_namespaces; cat /sys/kernel/security/lsm; cat /proc/sys/kernel/core_pattern; lscpu']);
  await command('apparmor', 'sudo', ['aa-status']);
  await command('apparmor-policy', 'sudo', ['bash', '-c', 'for path in /etc/apparmor.d/chrome /etc/apparmor.d/unprivileged_userns; do if test -f "$path"; then printf "\\n%s\\n" "$path"; cat "$path"; fi; done']);
  await command('kernel-before', 'sudo', ['dmesg', '--ctime']);
  await command('userns', 'unshare', ['--user', '--map-root-user', 'true']);
  await command('userns-network-pid', 'unshare', ['--user', '--map-root-user', '--net', '--pid', '--fork', 'true']);
  const packed = JSON.parse(await readFile(join(packageDirectory, 'packages.json'), 'utf8'));
  assert.equal(packed.manifest.chrome.buildId, '155.0.8059.39');
  for (const entry of packed.packages) assert.equal(digest(await readFile(join(packageDirectory, entry.filename))), entry.sha256);
  await save('packages.json', packed);
  scratch = await mkdtemp('/tmp/ea-');
  const app = join(scratch, 'a');
  await mkdir(app);
  const pin = packed.manifest.server.platforms['linux-arm64-gnu'];
  const sdkPackage = packed.packages.find(entry => entry.name === '@cbxss/eoka');
  const nativePackage = packed.packages.find(entry => entry.name === pin.packageName);
  assert.ok(sdkPackage && nativePackage);
  await writeFile(join(app, 'package.json'), JSON.stringify({ private: true, type: 'module', dependencies: { '@cbxss/eoka': `file:${join(packageDirectory, sdkPackage.filename)}`, [pin.packageName]: `file:${join(packageDirectory, nativePackage.filename)}` } }));
  const installed = await command('npm-install', 'npm', ['install', '--ignore-scripts', '--omit=optional', '--no-audit', '--no-fund'], { cwd: app, timeoutMs: 300000 });
  assert.equal(installed.code, 0);
  const require = createRequire(join(app, 'package.json'));
  const sdkImport = join(app, 'sdk-import.mjs');
  await writeFile(sdkImport, "export * from '@cbxss/eoka';\n");
  const { Browser, installBrowser, releaseManifest } = await import(pathToFileURL(sdkImport).href);
  assert.equal(releaseManifest.chrome.buildId, '155.0.8059.39');
  const server = join(dirname(require.resolve(`${pin.packageName}/package.json`)), pin.executablePath);
  const serverBytes = await readFile(server);
  assert.equal(digest(serverBytes), pin.sha256);
  assert.equal(serverBytes.length, pin.size);
  const cache = join(scratch, 'b');
  const chrome = await installBrowser({ browserCacheDir: cache, timeoutMs: 300000 });
  assert.equal(chrome.buildId, '155.0.8059.39');
  await save('chrome.json', chrome);
  const originalDigest = digest(await readFile(chrome.executablePath));
  const copiedDistribution = join(scratch, 'original');
  await cp(dirname(chrome.executablePath), copiedDistribution, { recursive: true, dereference: true });
  const copiedChrome = join(copiedDistribution, chrome.executablePath.split('/').at(-1));
  assert.equal(digest(await readFile(copiedChrome)), originalDigest);
  await save('copied-chrome.json', { buildId: chrome.buildId, releasePin: releaseManifest.chrome.platforms['linux-arm64-gnu'], originalExecutablePath: chrome.executablePath, executablePath: copiedChrome, executableSha256: originalDigest });
  await command('chrome-version', copiedChrome, ['--version']);
  await command('chrome-file', 'file', [copiedChrome]);
  await command('chrome-libraries', 'ldd', [copiedChrome]);
  await command('chrome-elf', 'readelf', ['-W', '-h', '-l', '-d', '-n', copiedChrome]);
  await command('chrome-resources', 'find', [copiedDistribution, '-maxdepth', '2', '-printf', '%M %u %g %s %p -> %l\n']);

  async function raw(name, executable) {
    const runtime = join(scratch, name);
    await mkdir(runtime);
    const args = ['--headless=new', '--no-first-run', '--no-default-browser-check', '--remote-debugging-port=0', `--user-data-dir=${join(runtime, 'p')}`, '--dump-dom', 'chrome://sandbox'];
    const env = { ...process.env, TMPDIR: runtime, TMP: runtime, TEMP: runtime };
    summary.probes[name] = await command(name, executable, args, { env, timeoutMs: 45000 });
    summary.probes[name].devtoolsObserved = /DevTools listening on ws:\/\//.test(await readFile(join(output, `${name}.stderr`), 'utf8'));
    await save('summary.json', summary);
  }

  await raw('raw-original', copiedChrome);
  const relocated = join(scratch, 'relocated');
  await mkdir(relocated);
  for (const entry of await readdir(copiedDistribution)) {
    const source = join(copiedDistribution, entry);
    const destination = join(relocated, entry);
    if (source === copiedChrome) await copyFile(source, destination);
    else await symlink(source, destination);
  }
  const relocatedChrome = join(relocated, copiedChrome.split('/').at(-1));
  await chmod(relocatedChrome, (await stat(copiedChrome)).mode & 0o777);
  assert.equal(digest(await readFile(relocatedChrome)), originalDigest);
  await raw('raw-relocated-unmodified', relocatedChrome);

  for (const ownedProfile of [true, false]) {
    const name = ownedProfile ? 'managed-patched' : 'managed-original-user-profile';
    const stderrPath = join(output, `${name}.server.stderr`);
    const wrapper = join(scratch, `${name}.sh`);
    await writeFile(wrapper, `#!/bin/sh\nexec ${quote(server)} "$@" 2>${quote(stderrPath)}\n`);
    await chmod(wrapper, 0o700);
    const runtime = join(scratch, ownedProfile ? 'r1' : 'r2');
    await mkdir(runtime);
    const options = { serverPath: wrapper, serverEnv: { RUST_LOG: 'eoka=trace,eoka_server=info', NO_COLOR: '1' }, browserCacheDir: cache, download: 'never', headless: true, sandbox: true, runtimeDir: runtime, viewport: { width: 800, height: 600, deviceScaleFactor: 1 }, timeoutMs: 60000 };
    if (!ownedProfile) {
      options.userDataDir = join(scratch, 'user-profile');
      options.executablePath = copiedChrome;
    }
    await save(`${name}.options.json`, options);
    let browser;
    try {
      browser = await Browser.launch(options);
      const page = await browser.newPage('chrome://sandbox');
      summary.probes[name] = { launched: true, sandboxPage: await page.evaluate('document.body.innerText'), hello: browser.hello };
    } catch (error) {
      summary.probes[name] = { launched: browser !== undefined, error: failure(error) };
    } finally {
      if (browser) {
        try { await browser.close(); } catch (error) { summary.probes[name].closeError = failure(error); }
      }
      await save('summary.json', summary);
    }
  }
  assert.equal(digest(await readFile(chrome.executablePath)), originalDigest);
  assert.equal(digest(await readFile(server)), pin.sha256);
  const verifiedCache = await installBrowser({ browserCacheDir: cache, download: 'never' });
  assert.equal(verifiedCache.source, 'cache');
  assert.equal(verifiedCache.buildId, chrome.buildId);
  assert.equal(verifiedCache.executablePath, chrome.executablePath);
  await save('cache-reverified.json', verifiedCache);
  summary.originalBytesUnchanged = true;
} catch (error) {
  summary.error = failure(error);
  process.exitCode = 1;
} finally {
  await command('kernel-after', 'sudo', ['dmesg', '--ctime']);
  await command('kernel-journal', 'sudo', ['journalctl', '-k', '--since', '-15 minutes', '--no-pager']);
  await save('summary.json', summary);
  if (scratch) await rm(scratch, { recursive: true, force: true });
}
