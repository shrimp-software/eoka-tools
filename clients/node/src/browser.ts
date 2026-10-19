import { access, chmod, mkdir, mkdtemp, rm, stat } from 'node:fs/promises';
import { constants } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import type { CallOptions, InvokeClient } from './client.js';
import { ConfigurationError, EokaError, ProtocolError, TransportError } from './errors.js';
import type { MethodName, MethodParams, MethodResults } from './generated/protocol.js';
import { Operation } from './operations.js';
import { Page, type PerformOptions } from './page.js';
import { Recording } from './recording.js';
import { SidecarTransport } from './transport.js';
import { installBrowser, type InstallProgress } from './browser-install.js';
import { resolveCloudflareOptions, type CloudflareOptions } from './cloudflare.js';
import { resolveServerBinary } from './server-binary.js';

export interface BrowserOptions {
  serverPath?: string;
  serverEnv?: NodeJS.ProcessEnv;
  signal?: AbortSignal;
  timeoutMs?: number;
  shutdownTimeoutMs?: number;
  /** Have the server watch every tab and solve full-page Cloudflare challenges. */
  autoSolveCloudflare?: boolean | CloudflareOptions;
}

export interface LaunchOptions extends BrowserOptions {
  executablePath?: string;
  headless?: boolean;
  sandbox?: boolean;
  viewport?: { width: number; height: number; deviceScaleFactor?: number };
  runtimeDir?: string;
  userDataDir?: string;
  browserCacheDir?: string;
  download?: 'auto' | 'never';
  onProgress?: (progress: InstallProgress) => void;
  userAgent?: string;
  timezone?: string;
  proxy?: NonNullable<MethodParams['browser.launch']['proxy']>;
}

export interface ConnectOptions extends BrowserOptions {
  browserWSEndpoint: string;
}

function validateTimeout(value: number | undefined, name: string): void {
  if (value !== undefined && (!Number.isFinite(value) || value <= 0 || value > 2_147_483_647)) {
    throw new ConfigurationError(`${name} must be finite, positive and at most 2147483647 milliseconds`);
  }
}

function pathOption(value: string, name: string): string {
  if (typeof value !== 'string' || value.trim() === '' || value.includes('\0')) {
    throw new ConfigurationError(`${name} must be a nonempty filesystem path`);
  }
  return resolve(value);
}

async function executablePath(value: string, name: string): Promise<string> {
  const absolute = pathOption(value, name);
  try {
    if (!(await stat(absolute)).isFile()) throw new Error('not a regular file');
    await access(absolute, process.platform === 'win32' ? constants.F_OK : constants.X_OK);
  } catch (cause) {
    throw new ConfigurationError(`${name} is not an accessible executable: ${absolute}`, { cause });
  }
  return absolute;
}

function checkAbort(signal: AbortSignal | undefined, requestState: 'not_sent' | 'acknowledged' = 'not_sent'): void {
  if (signal?.aborted) {
    const reason = signal.reason;
    throw new TransportError(reason instanceof TransportError ? reason.code : 'Aborted', reason instanceof TransportError ? reason.message : 'Browser lifecycle operation was aborted; cleanup is not canceled', { cause: reason, requestState });
  }
}

function remainingTimeout(deadline: number | undefined): number | undefined {
  if (deadline === undefined) return undefined;
  const remaining = deadline - performance.now();
  if (remaining <= 0) throw new TransportError('DeadlineExceeded', 'Browser lifecycle operation exceeded its local deadline', { requestState: 'not_sent' });
  return remaining;
}

function localOptions(options: CallOptions, deadline: number | undefined): CallOptions {
  const timeoutMs = remainingTimeout(deadline);
  const local: CallOptions = {};
  if (options.signal !== undefined) local.signal = options.signal;
  if (options.operationId !== undefined) local.operationId = options.operationId;
  if (timeoutMs !== undefined) local.timeoutMs = timeoutMs;
  return local;
}

function lifecycleScope(options: BrowserOptions): { options: BrowserOptions; deadline: number | undefined; dispose: () => void } {
  validateTimeout(options.timeoutMs, 'timeoutMs');
  validateTimeout(options.shutdownTimeoutMs, 'shutdownTimeoutMs');
  checkAbort(options.signal);
  const deadline = options.timeoutMs === undefined ? undefined : performance.now() + options.timeoutMs;
  const controller = new AbortController();
  const abort = () => controller.abort(new TransportError('Aborted', 'Browser lifecycle operation was aborted; cleanup is not canceled', { cause: options.signal?.reason, requestState: 'not_sent' }));
  options.signal?.addEventListener('abort', abort, { once: true });
  const timer = options.timeoutMs === undefined ? undefined : setTimeout(() => {
    controller.abort(new TransportError('DeadlineExceeded', 'Browser lifecycle operation exceeded its local deadline', { requestState: 'not_sent' }));
  }, options.timeoutMs);
  return {
    options: { ...options, signal: controller.signal },
    deadline,
    dispose: () => {
      clearTimeout(timer);
      options.signal?.removeEventListener('abort', abort);
    },
  };
}

function lifecycleFailure(primary: unknown, signal: AbortSignal | undefined, deadline: number | undefined): unknown {
  if (signal?.aborted && signal.reason instanceof TransportError && primary instanceof EokaError && ['Aborted', 'InstallationAborted', 'InstallationTimeout'].includes(primary.code)) {
    return new TransportError(signal.reason.code, signal.reason.message, { cause: primary, requestState: primary.requestState ?? 'not_sent', requestId: primary.requestId, stderr: primary.stderr });
  }
  if (deadline !== undefined && primary instanceof EokaError && primary.code === 'InstallationTimeout') {
    return new TransportError('DeadlineExceeded', 'Browser lifecycle operation exceeded its local deadline', { cause: primary, requestState: 'not_sent' });
  }
  return primary;
}

async function runtimeDirectory(root: string): Promise<string> {
  try {
    await mkdir(root, { recursive: true, mode: 0o700 });
    const directory = await mkdtemp(join(root, 'e-'));
    try {
      if (process.platform !== 'win32') await chmod(directory, 0o700);
      return directory;
    } catch (primary) {
      await removeRuntime(directory, primary);
      throw primary;
    }
  } catch (cause) {
    throw new ConfigurationError(`Could not create private browser runtime under ${root}`, { cause });
  }
}

async function removeRuntime(directory: string, primary?: unknown): Promise<void> {
  try {
    await rm(directory, { recursive: true, force: true });
  } catch (cause) {
    const cleanup = new TransportError('RuntimeCleanup', `Could not remove owned browser runtime ${directory}`, { cause });
    if (primary !== undefined) throw new AggregateError([primary, cleanup], 'Browser lifecycle failed and owned runtime cleanup failed', { cause: primary });
    throw cleanup;
  }
}

async function removeClosedRuntime(directory: string, transport?: SidecarTransport, primary?: unknown): Promise<void> {
  if (transport !== undefined && !transport.ownedProcessesTerminated) {
    const cleanup = new TransportError('RuntimeCleanupUncertain', `Owned browser runtime retained at ${directory}: sidecar process-tree termination could not be confirmed`, { data: { runtimeDir: directory } });
    if (primary !== undefined) throw new AggregateError([primary, cleanup], 'Browser lifecycle failed and owned runtime was retained because process cleanup is uncertain', { cause: primary });
    throw cleanup;
  }
  await removeRuntime(directory, primary);
}

function validateEndpoint(value: string): void {
  let endpoint: URL;
  try {
    endpoint = new URL(value);
  } catch {
    throw new ConfigurationError('browserWSEndpoint must be a ws:// browser DevTools endpoint');
  }
  if (typeof value !== 'string' || endpoint.protocol !== 'ws:' || !endpoint.hostname || endpoint.username || endpoint.password || value.includes('?') || value.includes('#') || !/^\/devtools\/browser\/[^/]+$/.test(endpoint.pathname)) {
    throw new ConfigurationError('browserWSEndpoint must be a ws:// browser DevTools endpoint without credentials, query or fragment; wss:// is unsupported');
  }
}

function launchParams(options: LaunchOptions): MethodParams['browser.launch'] {
  const params: MethodParams['browser.launch'] = { headless: options.headless ?? true, sandbox: options.sandbox ?? true, deviceScaleFactor: null };
  for (const name of ['headless', 'sandbox'] as const) {
    if (options[name] !== undefined && typeof options[name] !== 'boolean') throw new ConfigurationError(`${name} must be a boolean`);
  }
  for (const name of ['userAgent', 'timezone'] as const) {
    if (options[name] !== undefined) {
      if (typeof options[name] !== 'string') throw new ConfigurationError(`${name} must be a string`);
      params[name] = options[name];
    }
  }
  for (const name of ['runtimeDir', 'userDataDir'] as const) {
    if (options[name] !== undefined) params[name] = pathOption(options[name], name);
  }
  if (options.viewport !== undefined) {
    const viewport = options.viewport;
    if (!viewport || !Number.isInteger(viewport.width) || !Number.isInteger(viewport.height) || viewport.width < 1 || viewport.height < 1 || viewport.width > 2_147_483_647 || viewport.height > 2_147_483_647) {
      throw new ConfigurationError('viewport width and height must be positive integers at most 2147483647');
    }
    if (viewport.deviceScaleFactor !== undefined && (!Number.isFinite(viewport.deviceScaleFactor) || viewport.deviceScaleFactor <= 0)) {
      throw new ConfigurationError('viewport deviceScaleFactor must be finite and positive');
    }
    params.viewportWidth = viewport.width;
    params.viewportHeight = viewport.height;
    params.deviceScaleFactor = viewport.deviceScaleFactor ?? null;
  }
  if (options.proxy !== undefined) {
    const proxy = options.proxy;
    let endpoint: URL;
    try {
      endpoint = new URL(proxy.server);
    } catch {
      throw new ConfigurationError('proxy.server must be an http:// or socks5:// URL with host and port');
    }
    if (!['http:', 'socks5:'].includes(endpoint.protocol) || !endpoint.hostname || !endpoint.port || endpoint.username || endpoint.password || !['', '/'].includes(endpoint.pathname) || proxy.server.includes('?') || proxy.server.includes('#')) {
      throw new ConfigurationError('proxy.server must include host and port without credentials, path, query or fragment');
    }
    const username = proxy.username ?? null;
    const password = proxy.password ?? null;
    if ((username === null) !== (password === null) || (username !== null && typeof username !== 'string') || (password !== null && typeof password !== 'string')) {
      throw new ConfigurationError('proxy credentials require both username and password strings');
    }
    params.proxy = { server: proxy.server, username, password };
  }
  return params;
}

export class Browser implements InvokeClient {
  readonly #transport: SidecarTransport;
  readonly #sessionInfo: MethodResults['browser.launch'];
  readonly #capabilities: ReadonlySet<string>;
  readonly #shutdownTimeoutMs: number;
  #closePromise: Promise<void> | undefined;
  readonly #pageOwners = new Set<Promise<void>>();
  readonly #creationCleanupFailures: unknown[] = [];
  readonly #runtimeCleanup: Promise<void>;

  private constructor(transport: SidecarTransport, sessionInfo: MethodResults['browser.launch'], shutdownTimeoutMs: number, runtimeDir?: string) {
    this.#transport = transport;
    this.#sessionInfo = Object.freeze(sessionInfo);
    this.#capabilities = new Set(transport.hello.capabilities);
    this.#shutdownTimeoutMs = shutdownTimeoutMs;
    this.#runtimeCleanup = runtimeDir === undefined
      ? Promise.resolve()
      : transport.whenClosed().then(() => removeClosedRuntime(runtimeDir, transport), () => removeClosedRuntime(runtimeDir, transport));
    void this.#runtimeCleanup.catch(() => {});
  }

  static async launch(options: LaunchOptions = {}): Promise<Browser> {
    const params = launchParams(options);
    if (options.download !== undefined && !['auto', 'never'].includes(options.download)) throw new ConfigurationError("download must be 'auto' or 'never'");
    if (options.onProgress !== undefined && typeof options.onProgress !== 'function') throw new ConfigurationError('onProgress must be a function');
    const browserCacheDir = options.browserCacheDir === undefined ? undefined : pathOption(options.browserCacheDir, 'browserCacheDir');
    const env = { ...process.env, ...options.serverEnv };
    const root = pathOption(options.runtimeDir ?? env.EOKA_RUNTIME_DIR ?? tmpdir(), 'runtimeDir');
    const scope = lifecycleScope(options);
    let runtimeDir: string | undefined;
    const runtimeOwner: { transport?: SidecarTransport } = {};
    try {
      const server = await resolveServerBinary({ ...(options.serverPath === undefined ? {} : { serverPath: options.serverPath }), env, signal: scope.options.signal! });
      checkAbort(scope.options.signal);
      remainingTimeout(scope.deadline);
      const chromePath = options.executablePath ?? env.EOKA_CHROME_PATH;
      let expectedBrowserVersion: string | undefined;
      if (chromePath !== undefined) {
        params.executablePath = await executablePath(chromePath, 'executablePath');
      } else {
        const installation = await installBrowser({
          ...(browserCacheDir === undefined ? {} : { browserCacheDir }),
          ...(options.download === undefined ? {} : { download: options.download }),
          ...(options.onProgress === undefined ? {} : { onProgress: options.onProgress }),
          ...localOptions(scope.options, scope.deadline),
          env,
        });
        params.executablePath = installation.executablePath;
        expectedBrowserVersion = installation.buildId;
      }
      checkAbort(scope.options.signal);
      remainingTimeout(scope.deadline);
      runtimeDir = await runtimeDirectory(root);
      params.runtimeDir = runtimeDir;
      const childEnv = { ...env, TMPDIR: runtimeDir, TMP: runtimeDir, TEMP: runtimeDir };
      const redactionValues = [params.proxy?.username, params.proxy?.password].filter((value): value is string => typeof value === 'string' && value.length > 0);
      return await Browser.start(scope.options, server.executablePath, childEnv, 'browser.launch', params, scope.deadline, redactionValues, runtimeDir, server.expectedServerVersion, expectedBrowserVersion, runtimeOwner);
    } catch (primary) {
      const failure = lifecycleFailure(primary, scope.options.signal, scope.deadline);
      if (runtimeDir !== undefined) await removeClosedRuntime(runtimeDir, runtimeOwner.transport, failure);
      throw failure;
    } finally {
      scope.dispose();
    }
  }

  static async connect(options: ConnectOptions): Promise<Browser> {
    validateEndpoint(options.browserWSEndpoint);
    for (const name of ['executablePath', 'headless', 'sandbox', 'viewport', 'runtimeDir', 'browserCacheDir', 'download', 'onProgress', 'userDataDir', 'userAgent', 'timezone', 'proxy']) {
      if (Object.hasOwn(options, name)) throw new ConfigurationError(`connect does not accept the launch option ${name}`);
    }
    const scope = lifecycleScope(options);
    try {
      const env = { ...process.env, ...options.serverEnv };
      const server = await resolveServerBinary({ ...(options.serverPath === undefined ? {} : { serverPath: options.serverPath }), env, signal: scope.options.signal! });
      return await Browser.start(scope.options, server.executablePath, env, 'browser.connect', { wsEndpoint: options.browserWSEndpoint }, scope.deadline, [options.browserWSEndpoint], undefined, server.expectedServerVersion);
    } catch (primary) {
      throw lifecycleFailure(primary, scope.options.signal, scope.deadline);
    } finally {
      scope.dispose();
    }
  }

  private static async start<M extends 'browser.launch' | 'browser.connect'>(options: BrowserOptions, serverPath: string, env: NodeJS.ProcessEnv, method: M, params: MethodParams[M], deadline: number | undefined, redactionValues: readonly string[], runtimeDir?: string, expectedServerVersion?: string, expectedBrowserVersion?: string, runtimeOwner?: { transport?: SidecarTransport }): Promise<Browser> {
    checkAbort(options.signal);
    const transport = await SidecarTransport.start({
      serverPath,
      env,
      redactionValues,
      ...localOptions(options, deadline),
      ...(options.shutdownTimeoutMs === undefined ? {} : { shutdownTimeoutMs: options.shutdownTimeoutMs }),
      ...(runtimeOwner === undefined ? {} : { onTransport: (transport: SidecarTransport) => { runtimeOwner.transport = transport; } }),
    });
    try {
      if (expectedServerVersion !== undefined && transport.hello.serverVersion !== expectedServerVersion) {
        throw new ProtocolError(`Packaged server version mismatch: expected ${expectedServerVersion}, received ${transport.hello.serverVersion}`, { requestState: 'acknowledged' });
      }
      const cloudflare = resolveCloudflareOptions(options.autoSolveCloudflare);
      if (cloudflare !== undefined) {
        if (!transport.hasCapability('cloudflareAutoSolve')) throw new ConfigurationError('autoSolveCloudflare requires a server with the cloudflareAutoSolve capability');
        params = { ...params, autoSolveCloudflare: cloudflare };
      }
      const session = await transport.invoke(method, params, localOptions(options, deadline));
      if (!session || !['owned', 'borrowed'].includes(session.ownership) || typeof session.browserVersion !== 'string' || !session.browserVersion.trim() || (method === 'browser.connect' && session.ownership !== 'borrowed')) {
        throw new ProtocolError('Invalid browser session ownership or browserVersion', { requestState: 'acknowledged' });
      }
      if (expectedBrowserVersion !== undefined && session.browserVersion !== expectedBrowserVersion && session.browserVersion !== `Chrome/${expectedBrowserVersion}` && session.browserVersion !== `HeadlessChrome/${expectedBrowserVersion}`) {
        throw new ProtocolError(`Managed browser version mismatch: expected ${expectedBrowserVersion}, received ${session.browserVersion}`, { requestState: 'acknowledged' });
      }
      checkAbort(options.signal, 'acknowledged');
      return new Browser(transport, session, options.shutdownTimeoutMs ?? 45_000, runtimeDir);
    } catch (primary) {
      try {
        await transport.close();
      } catch (cleanup) {
        throw new AggregateError([primary, cleanup], 'Browser lifecycle failed and sidecar cleanup failed; browser cleanup may be uncertain', { cause: primary });
      }
      throw primary;
    }
  }

  get hello(): MethodResults['server.hello'] { return this.#transport.hello; }
  get sessionInfo(): MethodResults['browser.launch'] { return this.#sessionInfo; }
  get ownership(): MethodResults['browser.launch']['ownership'] { return this.#sessionInfo.ownership; }
  get browserVersion(): string { return this.#sessionInfo.browserVersion; }

  supports(method: string): boolean { return this.#transport.supports(method); }
  hasCapability(name: string): boolean { return this.#capabilities.has(name); }

  invoke<M extends MethodName>(method: M, params: MethodParams[M], options?: CallOptions): Promise<MethodResults[M]> {
    return this.#transport.invoke(method, params, options);
  }

  page(id: string): Page { return new Page(this, id); }
  operation(id: string): Operation { return new Operation(this, id); }
  recording(id: string): Recording { return new Recording(this, id); }

  async tabs(options?: CallOptions): Promise<MethodResults['browser.tabs']['tabs']> {
    return (await this.invoke('browser.tabs', {}, options)).tabs;
  }

  async pages(options?: CallOptions): Promise<Page[]> {
    return (await this.tabs(options)).map(tab => this.page(tab.id));
  }

  operations(params: MethodParams['operations.list'] = {}, options?: CallOptions): Promise<MethodResults['operations.list']> {
    return this.invoke('operations.list', params, options);
  }

  newPage(url?: string, options: PerformOptions = {}): Promise<Page> {
    const result = this.createPage(url, options);
    const owner = result.then(
      () => { this.#pageOwners.delete(owner); },
      () => { this.#pageOwners.delete(owner); },
    );
    this.#pageOwners.add(owner);
    return result;
  }

  private async createPage(url: string | undefined, options: PerformOptions): Promise<Page> {
    if (url !== undefined && (typeof url !== 'string' || !url.trim())) throw new ConfigurationError('newPage URL must be a nonempty string');
    validateTimeout(options.timeoutMs, 'timeoutMs');
    const deadline = options.timeoutMs === undefined ? undefined : performance.now() + options.timeoutMs;
    const tracked = this.#transport.invokeTracked('browser.new_page', {}, localOptions(options, deadline));
    let result: MethodResults['browser.new_page'];
    try {
      result = await tracked.result;
    } catch (primary) {
      if (primary instanceof Error && 'requestState' in primary && primary.requestState === 'not_sent') throw primary;
      const cleanup = this.cleanupAbandonedCreation(tracked.completion);
      try {
        await cleanup;
      } catch (failure) {
        this.#creationCleanupFailures.push(failure);
        throw new AggregateError([primary, failure], 'New page creation failed and late-created-tab cleanup failed', { cause: primary });
      }
      throw primary;
    }
    const page = this.page(result.pageId);
    try {
      if (url !== undefined) await page.goto(url, { ...options, ...localOptions(options, deadline) });
      checkAbort(options.signal, 'acknowledged');
      return page;
    } catch (primary) {
      try {
        const cleanup = await this.#transport.invokeCleanup('browser.close_tab', { pageId: page.id }, { timeoutMs: this.#shutdownTimeoutMs });
        if (cleanup.cleanupError) throw new TransportError('CleanupFailed', 'Created tab was closed but input cleanup failed', { data: cleanup });
      } catch (cleanup) {
        throw new AggregateError([primary, cleanup], 'New page failed and created-tab cleanup failed', { cause: primary });
      }
      throw primary;
    }
  }

  private async cleanupAbandonedCreation(completion: Promise<MethodResults['browser.new_page']>): Promise<void> {
    const deadline = performance.now() + this.#shutdownTimeoutMs;
    let timer: NodeJS.Timeout | undefined;
    let timedOut = false;
    try {
      const result = await Promise.race([
        completion,
        new Promise<never>((_, reject) => {
          timer = setTimeout(() => {
            timedOut = true;
            reject(new TransportError('CleanupUncertain', 'Abandoned page creation did not return its page ID within the cleanup deadline; a borrowed browser may retain the unknown tab'));
          }, this.#shutdownTimeoutMs);
        }),
      ]);
      const cleanup = await this.#transport.invokeCleanup('browser.close_tab', { pageId: result.pageId }, { timeoutMs: remainingTimeout(deadline) ?? this.#shutdownTimeoutMs });
      if (cleanup.cleanupError) throw new TransportError('CleanupFailed', 'Late-created tab was closed but input cleanup failed', { data: cleanup });
    } catch (cause) {
      const uncertainty = new TransportError('CleanupUncertain', 'Could not establish cleanup of an abandoned page creation', { cause });
      if (timedOut) {
        try {
          await this.#transport.close();
        } catch (cleanup) {
          throw new AggregateError([uncertainty, cleanup], 'Abandoned page creation and sidecar cleanup remain uncertain', { cause: uncertainty });
        }
      }
      throw uncertainty;
    } finally {
      clearTimeout(timer);
    }
  }

  close(): Promise<void> {
    this.#closePromise ??= (async () => {
      this.#transport.quiesce();
      await Promise.all(this.#pageOwners);
      let transportFailure: unknown;
      try {
        await this.#transport.close();
      } catch (failure) {
        transportFailure = failure;
      }
      let runtimeFailure: unknown;
      try {
        await this.#runtimeCleanup;
      } catch (failure) {
        runtimeFailure = failure;
      }
      const failures = [...this.#creationCleanupFailures];
      if (transportFailure !== undefined) failures.unshift(transportFailure);
      if (runtimeFailure !== undefined) failures.push(runtimeFailure);
      if (failures.length === 1 && this.#creationCleanupFailures.length === 0 && runtimeFailure === undefined) throw transportFailure;
      if (failures.length) throw new AggregateError(failures, 'Browser disposal includes resource cleanup failures', { cause: failures[0] });
    })();
    return this.#closePromise;
  }

  [Symbol.asyncDispose](): Promise<void> { return this.close(); }
}
