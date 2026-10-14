import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { execFile, spawn } from 'node:child_process';
import { createReadStream, createWriteStream } from 'node:fs';
import { chmod, cp, lstat, mkdir, mkdtemp, readdir, readlink, realpath, rm, writeFile } from 'node:fs/promises';
import { homedir, tmpdir } from 'node:os';
import { dirname, join, relative, resolve } from 'node:path';
import { once } from 'node:events';
import { promisify } from 'node:util';
import { setTimeout as delay } from 'node:timers/promises';
import { Browser, installBrowser, releaseManifest } from '../dist/index.js';

const exec = promisify(execFile);
const options = Object.fromEntries(process.argv.slice(2).reduce((pairs, value, index, values) => index % 2 === 0 ? [...pairs, [value.replace(/^--/, ''), values[index + 1]]] : pairs, []));
assert.equal(process.platform, 'darwin');
assert.ok(options.before && options.fixed && options.output, 'Required: --before PATH --fixed PATH --output PATH');
assert.equal(releaseManifest.chrome.buildId, '155.0.8059.39');
const output = resolve(options.output);
await mkdir(output, { recursive: true });
const save = (name, value) => writeFile(join(output, name), `${JSON.stringify(value, null, 2)}\n`);
const failure = error => ({ name: error.name, message: error.message, stack: error.stack, code: error.code, stderr: error.stderr, cause: error.cause instanceof Error ? failure(error.cause) : error.cause, errors: error instanceof AggregateError ? error.errors.map(failure) : undefined });
const quote = value => `'${value.replaceAll("'", "'\\''")}'`;
const started = Date.now();
const evidence = { schemaVersion: 1, platform: `darwin-${process.arch}`, toolsSourceSha: process.env.TOOLS_SOURCE_SHA, beforeCoreRef: process.env.BEFORE_CORE_REF, fixedCoreRef: process.env.FIXED_CORE_REF, chromeBuildId: releaseManifest.chrome.buildId, sandbox: true, cases: [] };
const scratch = await realpath(await mkdtemp(join(tmpdir(), 'eoka-macos-diagnostic-')));
const shortRoot = await realpath(await mkdtemp('/tmp/em-'));
const cache = join(scratch, 'verified-cache');

async function command(binary, args, name) {
  try {
    const result = await exec(binary, args, { timeout: 120000, maxBuffer: 32 * 1024 * 1024 });
    await writeFile(join(output, `${name}.stdout`), result.stdout);
    await writeFile(join(output, `${name}.stderr`), result.stderr);
    return { code: 0 };
  } catch (error) {
    await writeFile(join(output, `${name}.stdout`), error.stdout ?? '');
    await writeFile(join(output, `${name}.stderr`), error.stderr ?? '');
    return { code: error.code, signal: error.signal, error: error.message };
  }
}

async function inventory(root) {
  const entries = [];
  async function visit(path) {
    const info = await lstat(path);
    const entry = { path: relative(root, path), mode: info.mode & 0o7777 };
    if (info.isSymbolicLink()) entries.push({ ...entry, kind: 'symlink', target: await readlink(path) });
    else if (info.isDirectory()) {
      entries.push({ ...entry, kind: 'directory' });
      for (const name of (await readdir(path)).sort()) await visit(join(path, name));
    } else if (info.isFile()) {
      const hash = createHash('sha256');
      for await (const bytes of createReadStream(path)) hash.update(bytes);
      entries.push({ ...entry, kind: 'file', size: info.size, sha256: hash.digest('hex') });
    } else throw new Error(`Unsupported inventory entry ${path}`);
  }
  await visit(root);
  return entries;
}

async function signatures(root, label) {
  const results = [];
  async function visit(path) {
    const info = await lstat(path);
    if (!info.isDirectory() || info.isSymbolicLink()) return;
    if (path.endsWith('.app') || path.endsWith('.framework')) {
      const name = `${label}-signature-${results.length}`;
      results.push({ path: relative(root, path), display: await command('/usr/bin/codesign', ['--display', '--verbose=4', '--entitlements', ':-', path], name), verify: await command('/usr/bin/codesign', ['--verify', '--deep', '--strict', path], `${name}-verify`) });
    }
    for (const name of (await readdir(path)).sort()) await visit(join(path, name));
  }
  await visit(root);
  await save(`${label}-signatures.json`, results);
}

async function wrapper(server, label) {
  const path = join(scratch, `${label}-sidecar`);
  await writeFile(path, `#!/bin/bash\nexec ${quote(resolve(server))} 2>${quote(join(output, `${label}-sidecar.stderr`))}\n`);
  await chmod(path, 0o700);
  return path;
}

async function interact(browser) {
  const page = await browser.newPage('data:text/html,<title>Mac launch diagnostic</title><input id="field">');
  const receipt = await page.fill('#field', 'sandboxed native Mac launch');
  assert.equal(receipt.outcome, 'verified_complete');
  const value = await page.evaluate('document.querySelector("#field").value');
  assert.equal(value, 'sandboxed native Mac launch');
  return { ownership: browser.ownership, browserVersion: browser.browserVersion, receipt, value };
}

async function raw(executable, runtime, label) {
  const profile = join(runtime, 'raw-profile');
  await mkdir(profile);
  const args = ['--headless=new', '--remote-debugging-port=0', `--user-data-dir=${profile}`, '--no-first-run', '--no-default-browser-check', '--enable-logging=stderr', 'about:blank'];
  await save(`${label}-launch.json`, { executable, args, TMPDIR: runtime });
  const child = spawn(executable, args, { env: { ...process.env, TMPDIR: runtime, TMP: runtime, TEMP: runtime }, stdio: ['ignore', 'ignore', 'pipe'], detached: true });
  const exited = new Promise(resolveExit => child.once('close', (code, signal) => resolveExit({ code, signal })));
  const stream = createWriteStream(join(output, `${label}-chrome.stderr`));
  const streamClosed = once(stream, 'close');
  child.stderr.pipe(stream);
  let browser;
  try {
    const endpoint = await new Promise((resolveEndpoint, reject) => {
      let text = '';
      const timeout = setTimeout(() => reject(new Error('Raw Chrome did not announce DevTools within 30 seconds')), 30000);
      const onData = chunk => {
        text += chunk;
        const match = text.match(/DevTools listening on (ws:\/\/[^\r\n]+)/);
        if (match) { clearTimeout(timeout); child.stderr.off('data', onData); resolveEndpoint(match[1]); }
      };
      child.stderr.on('data', onData);
      child.once('error', error => { clearTimeout(timeout); reject(error); });
      child.once('close', (code, signal) => { clearTimeout(timeout); reject(new Error(`Raw Chrome closed: code=${code} signal=${signal}`)); });
    });
    browser = await Browser.connect({ serverPath: await wrapper(options.fixed, label), browserWSEndpoint: endpoint, timeoutMs: 60000, serverEnv: { RUST_LOG: 'info,eoka::cdp::transport::process=trace,eoka::stealth::patcher=debug' } });
    return await interact(browser);
  } finally {
    try {
      if (browser) await browser.close();
    } finally {
      if (child.exitCode === null && child.signalCode === null) {
        try { process.kill(-child.pid, 'SIGTERM'); } catch (error) { if (error.code !== 'ESRCH') throw error; }
        const timeout = setTimeout(() => { try { process.kill(-child.pid, 'SIGKILL'); } catch (error) { if (error.code !== 'ESRCH') throw error; } }, 10000);
        try { await exited; } finally { clearTimeout(timeout); }
      }
      await streamClosed;
    }
  }
}

let verifiedInventory;
let sourceBundle;
try {
  const installed = await installBrowser({ browserCacheDir: cache, timeoutMs: 300000 });
  evidence.installation = installed;
  sourceBundle = dirname(dirname(dirname(installed.executablePath)));
  assert.ok(sourceBundle.endsWith('.app'));
  verifiedInventory = await inventory(cache);
  await save('verified-cache-before.json', verifiedInventory);
  await signatures(sourceBundle, 'source');
  for (const entry of await inventory(sourceBundle)) {
    if (entry.kind !== 'symlink') continue;
    const target = await realpath(join(sourceBundle, entry.path));
    assert.ok(!entry.target.startsWith('/') && target.startsWith(`${sourceBundle}/`), `Disposable comparison cannot share external symlink ${entry.path}`);
  }
  for (const [rootKind, root] of [['short', shortRoot], ['normal', scratch]]) {
    for (const variant of ['raw', 'before', 'fixed']) {
      const label = `${variant}-${rootKind}`;
      const runtime = join(root, label);
      await mkdir(runtime);
      const entry = { variant, rootKind, runtime, status: 'failed' };
      let browser;
      let disposable;
      try {
        if (variant === 'raw') entry.interaction = await raw(installed.executablePath, runtime, label);
        else {
          let executable = installed.executablePath;
          if (variant === 'before') {
            disposable = join(root, `${label}-disposable`, sourceBundle.split('/').at(-1));
            await cp(sourceBundle, disposable, { recursive: true, dereference: false, verbatimSymlinks: true });
            executable = join(disposable, relative(sourceBundle, installed.executablePath));
            await save(`${label}-disposable-before.json`, await inventory(disposable));
          }
          browser = await Browser.launch({ serverPath: await wrapper(options[variant], label), executablePath: executable, headless: true, sandbox: true, runtimeDir: runtime, viewport: { width: 800, height: 600, deviceScaleFactor: 1 }, timeoutMs: 90000, serverEnv: { RUST_LOG: 'info,eoka::cdp::transport::process=trace,eoka::stealth::patcher=debug' } });
          entry.interaction = await interact(browser);
          await signatures(runtime, label);
        }
        entry.status = 'passed';
      } catch (error) { entry.error = failure(error); }
      finally {
        if (browser) {
          try { await browser.close(); } catch (error) { entry.cleanupError = failure(error); entry.status = 'failed'; }
        }
        if (disposable) {
          await save(`${label}-disposable-after.json`, await inventory(disposable));
          await signatures(disposable, `${label}-disposable`);
        }
        entry.runtimeEntriesAfterClose = await readdir(runtime);
        if (variant !== 'raw' && entry.runtimeEntriesAfterClose.length !== 0) entry.status = 'failed';
        const after = await inventory(cache);
        entry.verifiedCacheUnchanged = JSON.stringify(after) === JSON.stringify(verifiedInventory);
        if (!entry.verifiedCacheUnchanged) await save(`${label}-verified-cache-after.json`, after);
        evidence.cases.push(entry);
        await save('diagnosis.json', evidence);
      }
    }
  }
  evidence.offlineVerification = await installBrowser({ browserCacheDir: cache, download: 'never', timeoutMs: 300000 });
} catch (error) { evidence.error = failure(error); }
finally {
  await delay(5000);
  const reports = join(homedir(), 'Library/Logs/DiagnosticReports');
  try {
    for (const name of await readdir(reports)) {
      const path = join(reports, name);
      const info = await lstat(path);
      if (info.isFile() && info.mtimeMs >= started && /chrome|chromium|eoka/i.test(name)) await cp(path, join(output, name));
    }
  } catch (error) { evidence.crashReportError = failure(error); }
  await command('/bin/ps', ['-axo', 'pid=,args='], 'processes-after');
  evidence.status = !evidence.error && evidence.cases.length === 6 && evidence.cases.every(entry => entry.verifiedCacheUnchanged) && evidence.cases.filter(entry => entry.variant !== 'before').every(entry => entry.status === 'passed') ? 'passed' : 'failed';
  await save('diagnosis.json', evidence);
  await rm(shortRoot, { recursive: true, force: true });
  await rm(scratch, { recursive: true, force: true });
}
if (evidence.status !== 'passed') process.exitCode = 1;
