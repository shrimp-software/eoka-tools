import { fork, spawn, type ChildProcess } from 'node:child_process';
import { randomUUID } from 'node:crypto';
import { hostname, homedir } from 'node:os';
import { mkdir, readFile, readdir, rename, rm, writeFile } from 'node:fs/promises';
import path from 'node:path';
import { setTimeout as delay } from 'node:timers/promises';
import { InstallationError } from './install-errors.js';
import type { ChromePlatformRelease } from './release.js';
import type { PlatformId } from './platform.js';
import type { BrowserInstallation, InstallBrowserOptions } from './browser-install.js';
import type { WorkerMessage, WorkerRequest } from './browser-install-worker.js';
import { sanitizeInstallationDiagnostic } from './browser-install-diagnostics.js';

interface Owner {
  token: string;
  pid: number;
  host: string;
  processStart: string | null;
  workerPid: number | null;
  workerStart: string | null;
  ticket: number | null;
}
export interface InstallerRelease {
  platform: PlatformId;
  buildId: string;
  pin: ChromePlatformRelease;
}

export function browserCacheDirectory(options: InstallBrowserOptions): string {
  const env = options.env ?? process.env;
  if (options.browserCacheDir !== undefined) {
    if (!options.browserCacheDir.trim()) throw new InstallationError('InstallationFailed', 'browserCacheDir must not be empty');
    return path.resolve(options.browserCacheDir);
  }
  if (env.EOKA_BROWSER_CACHE_DIR) return path.resolve(env.EOKA_BROWSER_CACHE_DIR);
  if (process.platform === 'darwin') return path.join(homedir(), 'Library', 'Caches', 'eoka');
  if (process.platform === 'win32') return path.join(env.LOCALAPPDATA || path.join(homedir(), 'AppData', 'Local'), 'eoka', 'Cache');
  return path.join(env.XDG_CACHE_HOME || path.join(homedir(), '.cache'), 'eoka');
}

async function processStart(pid: number): Promise<string | null> {
  if (process.platform !== 'linux') return null;
  try {
    const stat = await readFile(`/proc/${pid}/stat`, 'utf8');
    return stat.slice(stat.lastIndexOf(')') + 2).split(' ')[19] ?? null;
  } catch { return null; }
}

async function alive(pid: number, start: string | null): Promise<boolean> {
  try { process.kill(pid, 0); } catch (error) { return (error as NodeJS.ErrnoException).code !== 'ESRCH'; }
  if (process.platform !== 'linux') return true;
  try {
    const stat = await readFile(`/proc/${pid}/stat`, 'utf8');
    const fields = stat.slice(stat.lastIndexOf(')') + 2).split(' ');
    return fields[0] !== 'Z' && fields[0] !== 'X' && (start === null || fields[19] === start);
  } catch (error) {
    return (error as NodeJS.ErrnoException).code !== 'ENOENT';
  }
}

async function stopWorker(child: ChildProcess): Promise<void> {
  if (!child.pid) return;
  if (process.platform === 'win32' && (child.exitCode !== null || child.signalCode !== null)) return;
  if (process.platform === 'win32') {
    await new Promise<void>((resolve, reject) => {
      const killer = spawn('taskkill', ['/PID', String(child.pid), '/T', '/F'], { stdio: 'ignore', windowsHide: true });
      killer.once('error', reject);
      killer.once('close', code => code === 0 || child.exitCode !== null ? resolve() : reject(new Error(`taskkill failed with code ${code}`)));
    });
  } else {
    try { process.kill(-child.pid, 'SIGKILL'); } catch (error) {
      if ((error as NodeJS.ErrnoException).code !== 'ESRCH') throw error;
    }
  }
}

async function runWorker(request: WorkerRequest, options: InstallBrowserOptions, signal: AbortSignal, onStarted?: (pid: number) => Promise<void>): Promise<boolean> {
  signal.throwIfAborted();
  const child = fork(new URL('./browser-install-worker.js', import.meta.url), [], {
    env: { ...(options.env ?? process.env) },
    stdio: ['ignore', 'ignore', 'pipe', 'ipc'],
    execArgv: [],
    detached: process.platform !== 'win32',
  });
  return await new Promise<boolean>((resolve, reject) => {
    let result: boolean | undefined;
    let failure: unknown;
    let stderr = '';
    let diagnosed = false;
    let stopping: Promise<void> | undefined;
    let starting: Promise<void> | undefined;
    const stop = (error: unknown): void => {
      failure ??= error;
      stopping ??= stopWorker(child).catch(cause => { failure = new InstallationError('InstallationFailed', 'Could not terminate browser installation worker', { cause }); });
    };
    const onAbort = (): void => stop(signal.reason);
    signal.addEventListener('abort', onAbort, { once: true });
    child.stderr?.on('data', (chunk: Buffer) => { stderr = (stderr + chunk.toString()).slice(-8192); });
    child.on('message', (message: WorkerMessage) => {
      if (message.type === 'progress') {
        try {
          if (options.onProgress) options.onProgress({ downloadedBytes: message.downloadedBytes, totalBytes: message.totalBytes, buildId: request.buildId });
          else if (!diagnosed) {
            diagnosed = true;
            process.stderr.write(`Downloading pinned Chrome ${request.buildId} to ${browserCacheDirectory(options)}\n`);
          }
        }
        catch (cause) { stop(new InstallationError('InstallationFailed', 'Browser installation progress callback failed', { cause })); }
      } else if (message.type === 'error') {
        failure ??= new InstallationError(message.code, sanitizeInstallationDiagnostic(message.message, options.env ?? process.env), { stderr: sanitizeInstallationDiagnostic(stderr, options.env ?? process.env) });
      } else if (message.type === 'result') result = message.verified;
    });
    child.once('error', cause => stop(new InstallationError('InstallationFailed', 'Could not start browser installation worker', { cause })));
    child.once('close', async code => {
      signal.removeEventListener('abort', onAbort);
      if (starting) await starting;
      if (stopping) await stopping;
      if (code !== 0 && !stopping) {
        try { await stopWorker(child); } catch (cause) { failure ??= cause; }
      }
      if (failure !== undefined) reject(failure);
      else if (code !== 0 || result === undefined) reject(new InstallationError('InstallationFailed', `Browser installation worker exited unexpectedly (${code})`, { stderr: sanitizeInstallationDiagnostic(stderr, options.env ?? process.env) }));
      else resolve(result);
    });
    starting = (async () => {
      try {
        if (child.pid && onStarted) await onStarted(child.pid);
        signal.throwIfAborted();
        child.send(request, error => { if (error) stop(error); });
      } catch (error) { stop(error); }
    })();
    if (signal.aborted) onAbort();
  });
}

async function readOwner(file: string): Promise<Owner | null> {
  try {
    const value = JSON.parse(await readFile(file, 'utf8')) as Owner;
    if (typeof value.token !== 'string' || !Number.isSafeInteger(value.pid) || value.pid <= 0 || typeof value.host !== 'string' || (value.ticket !== null && (!Number.isSafeInteger(value.ticket) || value.ticket <= 0))) throw new InstallationError('InstallationFailed', `Invalid browser lock owner: ${file}`);
    return value;
  } catch (error) {
    if ((error as NodeJS.ErrnoException).code === 'ENOENT') return null;
    throw error;
  }
}

async function writeOwner(lock: string, owner: Owner): Promise<void> {
  const temporary = path.join(lock, `${owner.token}.tmp`);
  await writeFile(temporary, JSON.stringify(owner), { flag: 'wx', mode: 0o600 });
  await rename(temporary, path.join(lock, `${owner.token}.json`));
}

async function contenders(lock: string): Promise<Owner[]> {
  const result: Owner[] = [];
  const host = hostname();
  for (const name of await readdir(lock)) {
    if (!name.endsWith('.json')) continue;
    const file = path.join(lock, name);
    const owner = await readOwner(file);
    if (!owner) continue;
    if (owner.host === host && !await alive(owner.pid, owner.processStart) && (owner.workerPid === null || !await alive(owner.workerPid, owner.workerStart))) {
      await rm(file, { force: true });
    } else result.push(owner);
  }
  return result;
}

async function acquireLock(lock: string, owner: Owner, signal: AbortSignal): Promise<void> {
  await mkdir(lock, { recursive: true, mode: 0o700 });
  signal.throwIfAborted();
  await writeOwner(lock, owner);
  let maximum = 0;
  for (const contender of await contenders(lock)) maximum = Math.max(maximum, contender.ticket ?? 0);
  if (maximum >= Number.MAX_SAFE_INTEGER) throw new InstallationError('InstallationFailed', 'Browser lock ticket space is exhausted');
  const ticket = maximum + 1;
  owner.ticket = ticket;
  await writeOwner(lock, owner);
  while (true) {
    signal.throwIfAborted();
    const waiting = (await contenders(lock)).some(other => other.token !== owner.token && (other.ticket === null || other.ticket < ticket || (other.ticket === ticket && other.token < owner.token)));
    if (!waiting) return;
    await delay(100, undefined, { signal });
  }
}

export function createBrowserInstaller(release: InstallerRelease): (options?: InstallBrowserOptions) => Promise<BrowserInstallation> {
  return async (options = {}) => {
    if (!release.pin.sha256 || !/^[a-f0-9]{64}$/.test(release.pin.sha256)) throw new InstallationError('ReleaseNotPrepared', 'The managed Chrome release has no reviewed archive checksum');
    if (!/^[0-9]+(?:\.[0-9]+){3}$/.test(release.buildId)) throw new InstallationError('ReleaseNotPrepared', 'Invalid managed Chrome build identifier');
    if (options.download !== undefined && options.download !== 'auto' && options.download !== 'never') throw new InstallationError('InstallationFailed', 'download must be auto or never');
    const timeoutMs = options.timeoutMs ?? 300_000;
    if (!Number.isFinite(timeoutMs) || timeoutMs <= 0 || timeoutMs > 2_147_483_647) throw new InstallationError('InstallationFailed', 'timeoutMs must be a positive finite duration no greater than 2147483647');
    const cacheDir = browserCacheDirectory(options);
    const controller = new AbortController();
    const abort = (): void => controller.abort(new InstallationError('InstallationAborted', 'Browser installation was aborted', { cause: options.signal?.reason }));
    options.signal?.addEventListener('abort', abort, { once: true });
    if (options.signal?.aborted) abort();
    const timer = setTimeout(() => controller.abort(new InstallationError('InstallationTimeout', `Browser installation exceeded ${timeoutMs}ms`)), timeoutMs);
    timer.unref();
    const signal = controller.signal;
    const key = `chrome-${release.platform}-${release.buildId}-${release.pin.sha256}`;
    const root = path.join(cacheDir, key);
    const executablePath = `chrome/${release.pin.platform}-${release.buildId}/${release.pin.executablePath}`;
    const request: WorkerRequest = { action: 'verify', root, ...release, executablePath };
    const lock = path.join(cacheDir, '.locks', key);
    const token = randomUUID();
    const stage = path.join(cacheDir, '.staging', `${key}-${token}`);
    let owner: Owner | undefined;
    let staged = false;
    try {
      if (await runWorker(request, options, signal)) return { executablePath: path.join(root, executablePath), cacheDir, buildId: release.buildId, platform: release.platform, source: 'cache' };
      signal.throwIfAborted();
      if (options.download === 'never') throw new InstallationError('BrowserNotInstalled', `Pinned Chrome ${release.buildId} is not installed in ${cacheDir}; run installBrowser() with downloads enabled before using offline mode`);
      const lockOwner: Owner = { token, pid: process.pid, host: hostname(), processStart: await processStart(process.pid), workerPid: null, workerStart: null, ticket: null };
      owner = lockOwner;
      await acquireLock(lock, lockOwner, signal);
      if (await runWorker(request, options, signal)) return { executablePath: path.join(root, executablePath), cacheDir, buildId: release.buildId, platform: release.platform, source: 'cache' };
      await mkdir(stage, { recursive: true, mode: 0o700 });
      staged = true;
      await runWorker({ ...request, action: 'install', root: stage }, options, signal, async pid => {
        lockOwner.workerPid = pid;
        lockOwner.workerStart = await processStart(pid);
        await writeOwner(lock, lockOwner);
      });
      signal.throwIfAborted();
      await rename(stage, root);
      staged = false;
      return { executablePath: path.join(root, executablePath), cacheDir, buildId: release.buildId, platform: release.platform, source: 'download' };
    } catch (cause) {
      if (signal.aborted) throw signal.reason;
      if (cause instanceof InstallationError) throw cause;
      throw new InstallationError('InstallationFailed', `Could not install pinned Chrome ${release.buildId}`, { cause });
    } finally {
      clearTimeout(timer);
      options.signal?.removeEventListener('abort', abort);
      try {
        if (staged) await rm(stage, { recursive: true, force: true });
        if (owner) {
          await rm(path.join(lock, `${token}.json`), { force: true });
          await rm(path.join(lock, `${token}.tmp`), { force: true });
        }
      } catch (cause) {
        throw new InstallationError('InstallationFailed', 'Could not clean up the owned browser installation stage or lock', { cause });
      }
    }
  };
}
