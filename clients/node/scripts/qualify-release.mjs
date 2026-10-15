import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { execFile } from 'node:child_process';
import { access, cp, mkdir, mkdtemp, readFile, realpath, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { delimiter, dirname, join, resolve } from 'node:path';
import { promisify } from 'node:util';
import { assertRecordingQualification, classifyRecordingDiagnostics, recordingSchedule } from './recording-qualification.mjs';

const exec = promisify(execFile);
const platforms = ['linux-x64-gnu', 'linux-arm64-gnu', 'darwin-x64', 'darwin-arm64', 'win32-x64'];
const digest = bytes => createHash('sha256').update(bytes).digest('hex');
const json = async path => JSON.parse(await readFile(path, 'utf8'));
const save = (path, value) => writeFile(path, `${JSON.stringify(value, null, 2)}\n`);

async function npm(args, cwd) {
  const options = { cwd, maxBuffer: 16 * 1024 * 1024, timeout: 300000, env: { ...process.env, npm_config_update_notifier: 'false' } };
  if (process.platform !== 'win32') return await exec('npm', args, options);
  let cli = process.env.npm_execpath;
  if (!cli) {
    for (const directory of (process.env.PATH ?? '').split(delimiter)) {
      try {
        const executable = await realpath(join(directory, 'npm.cmd'));
        const candidate = join(dirname(executable), 'node_modules', 'npm', 'bin', 'npm-cli.js');
        await access(candidate);
        cli = candidate;
        break;
      } catch (error) {
        if (!['ENOENT', 'ENOTDIR'].includes(error.code)) throw error;
      }
    }
  }
  assert.ok(cli, 'Cannot locate the npm JavaScript CLI on PATH');
  return await exec(process.execPath, [cli, ...args], options);
}

async function packStage(stage, output) {
  await mkdir(output, { recursive: true });
  const manifest = await json(join(stage, 'sdk', 'release-manifest.json'));
  const directories = [join(stage, 'sdk'), ...platforms.filter(id => manifest.server.platforms[id].sha256 !== null).map(id => join(stage, 'platforms', id))];
  const packages = [];
  for (const directory of directories) {
    const metadata = await json(join(directory, 'package.json'));
    const { stdout } = await npm(['pack', '--json', '--ignore-scripts', '--pack-destination', output], directory);
    const packed = JSON.parse(stdout);
    assert.equal(packed.length, 1);
    const filename = packed[0].filename;
    assert.equal(packed[0].name, metadata.name);
    assert.equal(packed[0].version, metadata.version);
    packages.push({ filename, name: metadata.name, version: metadata.version, sha256: digest(await readFile(join(output, filename))) });
  }
  await save(join(output, 'packages.json'), { manifest, packages });
  return { manifest, packages };
}

async function consumer(config, classifyRecordingDiagnostics) {
  const assert = (await import('node:assert/strict')).default;
  const { createHash } = await import('node:crypto');
  const { execFile, spawn } = await import('node:child_process');
  const { once } = await import('node:events');
  const { cp, mkdir, mkdtemp, readFile, readdir, realpath, rm, stat, writeFile } = await import('node:fs/promises');
  const { createRequire } = await import('node:module');
  const { dirname, join, resolve } = await import('node:path');
  const { setTimeout: delay } = await import('node:timers/promises');
  const { promisify } = await import('node:util');
  const exec = promisify(execFile);
  const sdk = await import('@cbxss/eoka');
  const { Browser, detectPlatform, installBrowser, releaseManifest } = sdk;
  const platform = detectPlatform();
  const evidence = { schemaVersion: 1, platform, sourceSha: config.sourceSha, status: 'failed', readiness: config.readiness, preview: config.preview, development: config.development, sandbox: true, checks: {}, recordings: [] };
  const save = (name, value) => writeFile(join(config.output, name), `${JSON.stringify(value, null, 2)}\n`);
  const failure = error => ({ name: error.name, message: error.message, stack: error.stack, code: error.code, stderr: error.stderr, data: error.data, recordingId: error.recordingId, record: error.record, failure: error.failure, path: error.path, partialPath: error.partialPath, cleanupUncertain: error.cleanupUncertain, cause: error.cause instanceof Error ? failure(error.cause) : error.cause, errors: error instanceof AggregateError ? error.errors.map(failure) : undefined });
  const command = async (binary, args, name) => {
    try {
      const result = await exec(binary, args, { maxBuffer: 32 * 1024 * 1024, timeout: 120000 });
      await writeFile(join(config.output, `${name}.stdout`), result.stdout);
      await writeFile(join(config.output, `${name}.stderr`), result.stderr);
      return result;
    } catch (error) {
      await writeFile(join(config.output, `${name}.stdout`), error.stdout ?? '');
      await writeFile(join(config.output, `${name}.stderr`), error.stderr ?? '');
      throw error;
    }
  };
  const runtime = join(config.scratch, 'r');
  let xvfb;
  let external;
  let externalExit;
  let active;
  try {
    for (const key of ['EOKA_SERVER_BIN', 'EOKA_CHROME_PATH', 'EOKA_RUNTIME_DIR']) assert.equal(process.env[key], undefined);
    const require = createRequire(import.meta.url);
    const pin = releaseManifest.server.platforms[platform];
    const nativeRoot = dirname(require.resolve(`${pin.packageName}/package.json`));
    const binary = join(nativeRoot, pin.executablePath);
    const metadataFile = join(nativeRoot, 'release-metadata.json');
    const metadataBytes = await readFile(metadataFile);
    const binaryBytes = await readFile(binary);
    assert.equal(createHash('sha256').update(binaryBytes).digest('hex'), pin.sha256);
    assert.equal(binaryBytes.length, pin.size);
    const badMetadata = JSON.parse(metadataBytes);
    badMetadata.serverVersion = '0.0.0-invalid';
    await writeFile(metadataFile, JSON.stringify(badMetadata));
    try {
      await assert.rejects(Browser.launch({ browserCacheDir: config.cache, download: 'never', timeoutMs: 30000 }), error => error.code === 'ServerPackageMismatch');
    } finally {
      await writeFile(metadataFile, metadataBytes);
    }
    const corrupt = Buffer.from(binaryBytes);
    corrupt[corrupt.length - 1] ^= 1;
    await writeFile(binary, corrupt);
    try {
      await assert.rejects(Browser.launch({ browserCacheDir: config.cache, download: 'never', timeoutMs: 30000 }), error => error.code === 'IntegrityMismatch');
    } finally {
      await writeFile(binary, binaryBytes);
    }
    evidence.checks.artifactFailFast = { metadataMismatch: true, binaryDigestMismatch: true, sha256: pin.sha256, size: pin.size, minimumGlibc: pin.minimumGlibc };
    await mkdir(config.cache, { recursive: true });
    const freshCache = await mkdtemp(join(config.cache, 'q-'));
    await assert.rejects(installBrowser({ browserCacheDir: freshCache, download: 'never' }), error => error.code === 'BrowserNotInstalled');
    let progress = 0;
    const fresh = await installBrowser({ browserCacheDir: freshCache, timeoutMs: 300000, onProgress: () => progress++ });
    assert.equal(fresh.source, 'download');
    assert.ok(progress > 0);
    const cached = await installBrowser({ browserCacheDir: freshCache, onProgress: () => { throw new Error('Cache unexpectedly downloaded Chrome'); } });
    assert.equal(cached.source, 'cache');
    const offline = await installBrowser({ browserCacheDir: freshCache, download: 'never', onProgress: () => { throw new Error('Offline mode unexpectedly downloaded Chrome'); } });
    assert.equal(offline.source, 'cache');
    assert.equal(offline.executablePath, fresh.executablePath);
    assert.equal(fresh.buildId, releaseManifest.chrome.buildId);
    evidence.checks.chrome = { fresh: fresh.source, cached: cached.source, offline: offline.source, missingOfflineRejected: true, buildId: fresh.buildId };
    await mkdir(runtime);
    const processes = async () => {
      const result = process.platform === 'win32'
        ? await exec('powershell.exe', ['-NoProfile', '-NonInteractive', '-Command', 'Get-CimInstance Win32_Process | Select-Object ProcessId,CommandLine | ConvertTo-Json -Compress'], { maxBuffer: 16 * 1024 * 1024 })
        : await exec('ps', ['-axo', 'pid=,args='], { maxBuffer: 16 * 1024 * 1024 });
      const rows = process.platform === 'win32' ? JSON.parse(result.stdout).map(row => [row.ProcessId, row.CommandLine ?? '']) : result.stdout.split('\n').map(line => { const match = line.match(/^\s*(\d+)\s+(.*)$/); return match ? [Number(match[1]), match[2]] : [0, '']; });
      return rows.filter(([pid, args]) => pid !== process.pid && args.includes(runtime)).map(([pid]) => pid);
    };
    const closeOwned = async () => {
      const pids = await processes();
      assert.ok(pids.length > 0, 'Could not observe owned browser processes');
      await active.close();
      active = undefined;
      assert.deepEqual(await readdir(runtime), [], 'Owned runtime was not removed');
      for (const pid of pids) {
        let alive = true;
        for (let attempt = 0; attempt < 50 && alive; attempt++) {
          try { process.kill(pid, 0); await delay(100); } catch (error) { if (error.code === 'ESRCH') alive = false; else throw error; }
        }
        assert.equal(alive, false, `Owned process ${pid} survived close`);
      }
      return pids;
    };
    // Navigation verifies a commit, not completion of HTML parsing.
    const waitForDocument = page => page.evaluate(`new Promise(resolve => {
      if (document.readyState !== 'loading') resolve(true);
      else document.addEventListener('DOMContentLoaded', () => resolve(true), { once: true });
    })`);
    const launch = headless => Browser.launch({ browserCacheDir: freshCache, download: 'never', headless, runtimeDir: runtime, viewport: { width: 800, height: 600, deviceScaleFactor: 1 }, timeoutMs: 60000 });
    active = await launch(true);
    assert.equal(active.ownership, 'owned');
    assert.equal(active.hello.serverVersion, releaseManifest.server.version);
    assert.equal(`${active.hello.protocol.major}.${active.hello.protocol.minor}`, releaseManifest.server.protocolVersion);
    evidence.checks.hello = active.hello;
    if (platform.startsWith('linux-')) {
      const sandboxPage = await active.newPage('chrome://sandbox');
      await waitForDocument(sandboxPage);
      const status = await sandboxPage.evaluate('document.body.innerText');
      for (const field of ['PID namespaces', 'Network namespaces', 'Seccomp-BPF sandbox']) {
        assert.match(status, new RegExp(`${field}\\s+Yes(?:\\r?\\n|$)`));
      }
      await sandboxPage.close();
      evidence.checks.sandbox = { status };
    }
    const scene = '<!doctype html><title>Qualification</title><style>html,body{margin:0;width:800px;height:1200px;background:#123;color:white}input{margin:20px}.box{width:80px;height:80px;background:orange;animation:move 1s alternate infinite linear}@keyframes move{from{transform:translateX(0)}to{transform:translateX(600px)}}</style><input id="field"><div class="box"></div>';
    const page = await active.newPage(`data:text/html,${encodeURIComponent(scene)}`);
    const receipt = await page.fill('#field', 'packed native qualification');
    assert.equal(receipt.outcome, 'verified_complete');
    assert.equal(receipt.dispatch, 'acknowledged');
    assert.equal(await page.evaluate('document.querySelector("#field").value'), 'packed native qualification');
    evidence.checks.fill = receipt;
    const screenshots = [];
    for (const format of ['png', 'jpeg', 'webp']) {
      for (const fullPage of [false, true]) {
        const name = `${format}-${fullPage ? 'full' : 'clip'}`;
        const path = join(config.output, `${name}.${format}`);
        const artifact = await page.screenshotToFile({ path, format, ...(fullPage ? { fullPage: true } : { clip: { x: 0, y: 0, width: 400, height: 300 } }) });
        assert.equal(await realpath(artifact.path), await realpath(path));
        assert.equal(artifact.bytes, (await stat(path)).size);
        assert.equal(artifact.mimeType, { png: 'image/png', jpeg: 'image/jpeg', webp: 'image/webp' }[format]);
        const { stdout } = await command('ffprobe', ['-v', 'error', '-select_streams', 'v:0', '-show_entries', 'stream=codec_name,width,height', '-of', 'json', path], name);
        const stream = JSON.parse(stdout).streams[0];
        assert.equal(stream.codec_name, { png: 'png', jpeg: 'mjpeg', webp: 'webp' }[format]);
        assert.equal(stream.width, fullPage ? 800 : 400);
        assert.equal(stream.height, fullPage ? 1200 : 300);
        screenshots.push({ format, fullPage, artifact, stream });
      }
    }
    const image = await page.screenshot({ format: 'png', clip: { x: 0, y: 0, width: 400, height: 300 } });
    assert.deepEqual([...image.subarray(0, 8)], [137, 80, 78, 71, 13, 10, 26, 10]);
    assert.equal(image.readUInt32BE(16), 400);
    assert.equal(image.readUInt32BE(20), 300);
    evidence.checks.screenshots = screenshots;
    const ffmpegVersion = await command('ffmpeg', ['-version'], 'ffmpeg-version');
    const ffmpegMajor = Number(ffmpegVersion.stdout.match(/^ffmpeg version (?:n)?(\d+)/)?.[1]);
    assert.ok(Number.isInteger(ffmpegMajor) && ffmpegMajor >= 4, 'Cannot establish supported ffmpeg version');
    const nativeTimeBase = ffmpegMajor >= 6 ? ['-fps_mode', 'passthrough', '-enc_time_base', 'demux'] : ffmpegMajor >= 5 ? ['-fps_mode', 'passthrough', '-enc_time_base', '-1'] : ['-vsync', '0', '-enc_time_base', '-1'];
    const recordingSchedule = config.recordingSchedule;
    evidence.checks.nativeRecordings = { artifactKind: 'finalized SDK MP4', browserVersion: active.browserVersion, schedule: recordingSchedule, attemptsPerCase: 1 };
    const recordingErrors = [];
    for (const scheduled of recordingSchedule) {
      const path = join(config.output, `${scheduled.name}.mp4`);
      const entry = { ...scheduled, path, artifactKind: 'finalized SDK MP4', status: 'failed' };
      const caseErrors = [];
      evidence.recordings.push(entry);
      let recording;
      try {
        recording = await page.record({ path, maxWidth: 800, maxHeight: 600, ...(scheduled.fps === undefined ? {} : { fps: scheduled.fps }) });
        entry.recordingId = recording.id;
        const started = performance.now();
        await delay(scheduled.waitMs);
        entry.elapsedBeforeStopMs = performance.now() - started;
        const stopping = performance.now();
        entry.artifact = await recording.stop({ timeoutMs: 60000 });
        entry.finalizationMs = performance.now() - stopping;
        entry.record = recording.record;
        assert.equal(entry.artifact.bytes, (await stat(path)).size);
        assert.equal(entry.artifact.container, 'mp4');
        assert.equal(entry.artifact.mimeType, 'video/mp4');
        try {
          const { stdout, stderr } = await command('ffprobe', ['-v', 'warning', '-select_streams', 'v:0', '-show_streams', '-show_packets', '-show_data_hash', 'sha256', '-of', 'json', path], `${scheduled.name}-probe`);
          entry.probeStderr = stderr;
          const probe = JSON.parse(stdout);
          entry.stream = probe.streams[0];
          entry.sampleCount = probe.packets.length;
          entry.packetDts = probe.packets.map(packet => packet.dts);
          entry.packetDurations = probe.packets.map(packet => packet.duration ?? null);
          entry.terminalSamples = probe.packets.slice(-3);
          assert.ok(probe.packets.length >= (scheduled.waitMs >= 1000 ? 2 : 1));
          assert.ok(entry.stream.width > 0 && entry.stream.width <= 800);
          assert.ok(entry.stream.height > 0 && entry.stream.height <= 600);
          assert.ok(Number(entry.stream.duration) > 0);
          const nonmonotonic = [];
          for (let packet = 0; packet < probe.packets.length; packet++) {
            assert.match(String(probe.packets[packet].dts), /^-?\d+$/);
            if (packet && BigInt(probe.packets[packet].dts) <= BigInt(probe.packets[packet - 1].dts)) nonmonotonic.push(packet);
          }
          entry.nonmonotonicDts = nonmonotonic;
          assert.deepEqual(nonmonotonic, [], 'Finalized SDK recording has duplicate or nonmonotonic packet DTS');
        } catch (error) {
          caseErrors.push(error);
        }
        try {
          const decoded = await command('ffmpeg', ['-nostdin', '-v', 'warning', '-xerror', '-i', path, '-map', '0:v:0', ...nativeTimeBase, '-f', 'null', '-'], `${scheduled.name}-decode`);
          entry.decodeStderr = decoded.stderr;
        } catch (error) {
          entry.decodeStderr = error.stderr ?? entry.decodeStderr;
          caseErrors.push(error);
        }
        if (caseErrors.length === 0) entry.advisories = classifyRecordingDiagnostics(entry);
      } catch (error) {
        entry.recordingId ??= error.recordingId;
        entry.record = recording?.record ?? error.record;
        caseErrors.push(error);
        if (error.partialPath) {
          entry.partialPath = join(config.output, `${scheduled.name}.partial.mp4`);
          try { await cp(error.partialPath, entry.partialPath); } catch (copyError) { caseErrors.push(copyError); }
        }
      }
      entry.errors = caseErrors.map(failure);
      if (caseErrors.length === 0) entry.status = 'passed';
      else recordingErrors.push(...caseErrors);
      await save('qualification.json', evidence);
    }
    evidence.checks.ownedCleanup = { pids: await closeOwned(), runtimeRemoved: true };
    if (process.platform === 'linux' && !process.env.DISPLAY) {
      xvfb = spawn('Xvfb', ['-displayfd', '3', '-screen', '0', '1280x1024x24', '-nolisten', 'tcp'], { stdio: ['ignore', 'ignore', 'pipe', 'pipe'] });
      let stderr = '';
      xvfb.stderr.on('data', bytes => { stderr += bytes; });
      const display = await new Promise((resolve, reject) => {
        const timer = setTimeout(() => reject(new Error(`Xvfb did not start: ${stderr}`)), 15000);
        xvfb.once('error', error => { clearTimeout(timer); reject(error); });
        xvfb.once('exit', code => { clearTimeout(timer); reject(new Error(`Xvfb exited ${code}: ${stderr}`)); });
        xvfb.stdio[3].once('data', bytes => { clearTimeout(timer); resolve(bytes.toString().trim()); });
      });
      assert.match(display, /^\d+$/);
      process.env.DISPLAY = `:${display}`;
    }
    active = await launch(false);
    const headed = await active.newPage('data:text/html,<title>Headed qualification</title>');
    await waitForDocument(headed);
    assert.equal(await headed.evaluate('document.title'), 'Headed qualification');
    evidence.checks.headed = { passed: true, pids: await closeOwned(), display: process.env.DISPLAY ?? null };
    const chromePin = releaseManifest.chrome.platforms[platform];
    const distribution = resolve(fresh.executablePath, ...chromePin.executablePath.split('/').map(() => '..'));
    const externalRoot = join(config.scratch, 'c');
    await cp(distribution, externalRoot, { recursive: true, verbatimSymlinks: true });
    const executable = join(externalRoot, chromePin.executablePath);
    const profile = join(config.scratch, 'p');
    await mkdir(profile);
    external = spawn(executable, ['--headless=new', '--remote-debugging-port=0', '--remote-debugging-address=127.0.0.1', `--user-data-dir=${profile}`, '--no-first-run', '--no-default-browser-check', 'about:blank'], { stdio: ['ignore', 'ignore', 'pipe'] });
    let externalStderr = '';
    external.stderr.on('data', bytes => { externalStderr += bytes; });
    external.on('error', error => { externalStderr += error.message; });
    await once(external, 'spawn');
    externalExit = once(external, 'exit');
    let port;
    for (let attempt = 0; attempt < 300; attempt++) {
      try {
        port = (await readFile(join(profile, 'DevToolsActivePort'), 'utf8')).split('\n')[0];
        break;
      } catch (error) {
        if (error.code !== 'ENOENT' && !(process.platform === 'win32' && error.code === 'EBUSY')) throw error;
      }
      assert.equal(external.exitCode, null, `External Chrome exited: ${externalStderr}`);
      await delay(100);
    }
    assert.match(port ?? '', /^\d+$/);
    const endpoint = `http://127.0.0.1:${port}/json/version`;
    const version = await (await fetch(endpoint)).json();
    active = await Browser.connect({ browserWSEndpoint: version.webSocketDebuggerUrl, timeoutMs: 60000 });
    assert.equal(active.ownership, 'borrowed');
    const borrowed = await active.newPage('data:text/html,<title>Borrowed qualification</title>');
    await waitForDocument(borrowed);
    assert.equal(await borrowed.evaluate('document.title'), 'Borrowed qualification');
    await active.close();
    active = undefined;
    assert.equal(external.exitCode, null);
    assert.ok((await fetch(endpoint)).ok, 'Borrowed Chrome DevTools endpoint did not survive');
    evidence.checks.borrowedSurvival = { pid: external.pid, endpointSurvived: true };
    assert.equal((await installBrowser({ browserCacheDir: freshCache, download: 'never' })).source, 'cache', 'Launching copied external Chrome mutated the verified distribution');
    if (recordingErrors.length > 0) throw new AggregateError(recordingErrors, 'Finalized SDK recording qualification failed; every scheduled case was attempted once');
    evidence.status = 'passed';
    if (config.readiness.status !== 'ready' || config.readiness.blockers.length !== 0) {
      evidence.status = 'blocked';
      throw new Error('Runtime checks completed, but source publication readiness remains blocked. Passing samples do not resolve known native recording defects.');
    }
  } catch (error) {
    evidence.error = failure(error);
    throw error;
  } finally {
    try {
      if (active) await active.close();
      if (external && external.exitCode === null) {
        if (process.platform === 'win32') await exec('taskkill.exe', ['/pid', String(external.pid), '/t', '/f']);
        else external.kill('SIGTERM');
        await Promise.race([externalExit, delay(10000).then(() => { throw new Error('External Chrome cleanup timed out'); })]);
      }
      if (xvfb && xvfb.exitCode === null) {
        const exited = once(xvfb, 'exit');
        xvfb.kill('SIGTERM');
        await exited;
      }
    } catch (error) {
      evidence.status = 'failed';
      evidence.cleanupError = failure(error);
      throw error;
    } finally {
      await save('qualification.json', evidence);
    }
  }
}

const options = {};
for (let index = 2; index < process.argv.length; index++) {
  const arg = process.argv[index];
  if (['--pack-only', '--aggregate', '--preview'].includes(arg)) options[arg.slice(2)] = true;
  else if (['--stage', '--cache', '--output', '--packages', '--evidence'].includes(arg)) {
    const value = process.argv[++index];
    if (!value || value.startsWith('--')) throw new Error(`Missing value for ${arg}`);
    options[arg.slice(2)] = resolve(value);
  } else throw new Error(`Unknown argument: ${arg}`);
}
if (!options.output) throw new Error('Usage: node qualify-release.mjs --stage DIR --cache DIR --output DIR [--packages DIR] [--preview (local host-development artifacts only)]');
await mkdir(options.output, { recursive: true });
if (options.aggregate) {
  assert.ok(options.packages && options.evidence);
  const { manifest, packages } = await json(join(options.packages, 'packages.json'));
  const readiness = await json(new URL('publication-readiness.json', import.meta.url));
  assert.equal(readiness.status, 'ready', 'Known release blocker remains unresolved');
  assert.deepEqual(readiness.blockers, []);
  assert.equal(readiness.sdkVersion, manifest.sdkVersion);
  assert.equal(readiness.chromeBuildId, manifest.chrome.buildId);
  assert.equal(packages.length, 6);
  const evidence = [];
  for (const platform of platforms) {
    const filename = `evidence/${platform}/qualification.json`;
    const source = join(options.evidence, platform, 'qualification.json');
    const result = await json(source);
    assert.equal(result.platform, platform);
    assert.equal(result.sourceSha, process.env.EOKA_SOURCE_SHA);
    assert.equal(result.status, 'passed');
    assert.equal(result.preview, false, 'Local preview cannot qualify a publication');
    assert.equal(result.development, false, 'Host-development artifact cannot qualify a publication');
    assert.equal(result.sandbox, true);
    assertRecordingQualification(result, manifest.chrome.buildId);
    assert.deepEqual(result.packages, packages);
    await cp(join(options.evidence, platform), join(options.output, 'evidence', platform), { recursive: true });
    evidence.push({ platform, filename, sha256: digest(await readFile(source)) });
  }
  for (const entry of packages) {
    assert.equal(digest(await readFile(join(options.packages, entry.filename))), entry.sha256);
    await cp(join(options.packages, entry.filename), join(options.output, entry.filename));
  }
  const publication = process.env.EOKA_PUBLICATION === 'true';
  const coreRef = process.env.EOKA_CORE_REF || null;
  assert.ok(!publication || coreRef === null);
  await save(join(options.output, 'qualification.json'), { schemaVersion: 1, sourceSha: process.env.EOKA_SOURCE_SHA, coreRef, publication, readiness, sdkVersion: manifest.sdkVersion, serverVersion: manifest.server.version, protocolVersion: manifest.server.protocolVersion, packages, platforms, evidence });
} else if (options['pack-only']) {
  assert.ok(options.stage);
  await packStage(options.stage, options.output);
} else {
  assert.ok(options.cache && (options.stage || options.packages));
  const packageDirectory = options.packages ?? join(options.output, 'packages');
  const packed = options.packages ? await json(join(packageDirectory, 'packages.json')) : await packStage(options.stage, packageDirectory);
  for (const entry of packed.packages) assert.equal(digest(await readFile(join(packageDirectory, entry.filename))), entry.sha256);
  const platform = process.platform === 'linux' ? `linux-${process.arch}-gnu` : `${process.platform}-${process.arch}`;
  assert.ok(platforms.includes(platform), `Unsupported qualification host ${platform}`);
  const hostPin = packed.manifest.server.platforms[platform];
  assert.ok(!hostPin.development || options.preview === true, 'Host-development artifacts require explicit --preview; this does not establish the glibc 2.35 release baseline');
  assert.ok(options.preview || (hostPin.minimumGlibc === null || hostPin.minimumGlibc === '2.35'), 'Release qualification requires the glibc 2.35 artifact baseline');
  if (process.getuid) assert.notEqual(process.getuid(), 0, 'Sandboxed qualification must run as a non-root user');
  const sdkPackage = packed.packages.find(entry => entry.name === '@cbxss/eoka');
  const nativePackage = packed.packages.find(entry => entry.name === hostPin.packageName);
  assert.ok(sdkPackage && nativePackage);
  const scratch = await mkdtemp(join(process.platform === 'win32' ? resolve(process.env.SystemDrive || 'C:', '/q') : tmpdir(), 'eq-')).catch(async error => {
    if (process.platform !== 'win32' || error.code !== 'ENOENT') throw error;
    await mkdir(resolve(process.env.SystemDrive || 'C:', '/q'), { recursive: true });
    return await mkdtemp(join(resolve(process.env.SystemDrive || 'C:', '/q'), 'eq-'));
  });
  const env = { ...process.env };
  for (const key of ['EOKA_SERVER_BIN', 'EOKA_CHROME_PATH', 'EOKA_RUNTIME_DIR']) delete env[key];
  const pathKey = Object.keys(env).find(key => key.toLowerCase() === 'path') ?? 'PATH';
  env[pathKey] = (env[pathKey] ?? '').split(delimiter).filter(directory => !/(?:cargo|rustup|rust-toolchain|[\\/]target[\\/])/i.test(directory)).join(delimiter);
  env.CARGO_HOME = join(scratch, 'no-cargo');
  try {
    const readiness = await json(new URL('publication-readiness.json', import.meta.url));
    assert.equal(readiness.schemaVersion, 1);
    assert.equal(readiness.sdkVersion, packed.manifest.sdkVersion);
    assert.equal(readiness.chromeBuildId, packed.manifest.chrome.buildId);
    assert.ok(Array.isArray(readiness.blockers));
    await save(join(options.output, 'qualification.json'), { schemaVersion: 1, platform, sourceSha: env.EOKA_SOURCE_SHA ?? null, readiness, preview: options.preview === true, development: hostPin.development, sandbox: true, packages: packed.packages, status: 'failed', phase: 'installation' });
    const app = join(scratch, 'a');
    await mkdir(app);
    await save(join(app, 'package.json'), { private: true, type: 'module', dependencies: { '@cbxss/eoka': `file:${join(packageDirectory, sdkPackage.filename)}`, [hostPin.packageName]: `file:${join(packageDirectory, nativePackage.filename)}` } });
    const install = await npm(['install', '--ignore-scripts', '--omit=optional', '--no-audit', '--no-fund'], app);
    await writeFile(join(options.output, 'npm-install.stdout'), install.stdout);
    await writeFile(join(options.output, 'npm-install.stderr'), install.stderr);
    const config = { scratch, cache: options.cache, output: options.output, sourceSha: env.EOKA_SOURCE_SHA ?? null, readiness, recordingSchedule, preview: options.preview === true, development: hostPin.development };
    await writeFile(join(app, 'qualify.mjs'), `await (${consumer.toString()})(${JSON.stringify(config)}, (${classifyRecordingDiagnostics.toString()}));\n`);
    try {
      const result = await exec(process.execPath, [join(app, 'qualify.mjs')], { cwd: app, env, maxBuffer: 32 * 1024 * 1024, timeout: 900000 });
      await writeFile(join(options.output, 'consumer.stdout'), result.stdout);
      await writeFile(join(options.output, 'consumer.stderr'), result.stderr);
    } catch (error) {
      await writeFile(join(options.output, 'consumer.stdout'), error.stdout ?? '');
      await writeFile(join(options.output, 'consumer.stderr'), error.stderr ?? '');
      throw error;
    } finally {
      const file = join(options.output, 'qualification.json');
      try { const evidence = await json(file); evidence.packages = packed.packages; await save(file, evidence); } catch (error) { if (error.code !== 'ENOENT') throw error; }
    }
  } catch (error) {
    const file = join(options.output, 'qualification.json');
    let evidence;
    try { evidence = await json(file); } catch (readError) { if (readError.code !== 'ENOENT') throw readError; }
    evidence ??= { schemaVersion: 1, platform, sourceSha: env.EOKA_SOURCE_SHA ?? null, packages: packed.packages, status: 'failed' };
    evidence.error ??= { name: error.name, message: error.message, code: error.code, stack: error.stack, stdout: error.stdout, stderr: error.stderr };
    await save(file, evidence);
    throw error;
  } finally {
    await rm(scratch, { recursive: true, force: true });
  }
}
