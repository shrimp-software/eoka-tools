import { catalogOperations, type CatalogOperations } from './generated/catalog.js';
import bundle from './generated/schema-bundle.json' with { type: 'json' };
import { Buffer } from 'node:buffer';
import { randomUUID } from 'node:crypto';
import { resolve } from 'node:path';
import { actionFailureMessage } from './action-failure.js';
import { traceCallSites } from './call-site.js';
import type { CallOptions, InvokeClient } from './client.js';
import { duration, maximumTimer, serverGraceMs } from './duration.js';
import { ConfigurationError, EokaError, type ErrorDetails } from './errors.js';
import type {
  Action, ActionOptions as WireActionOptions, ActionReceipt, AgentAction, AgentMode, AgentObservation,
  AgentStepResult, Artifact, BrowserState, Expectation, LocatorSpec, MethodName,
  MethodParams, MethodResults, OperationRecord, InputMode, Destination, ScreenshotClip, ScreenshotFormat,
} from './generated/protocol.js';
import { Operation, OperationError, operationFailure, validateOperationRecord, type WaitOptions } from './operations.js';
import { Recording, RecordingError, recordingFailure, requireCaptureCapability, validateRecordingRecord } from './recording.js';

export type LocatorInput = string | LocatorSpec;

type Declared<T> = { [K in keyof T as string extends K ? never : number extends K ? never : K]: T[K] };
export type ActionOptions = Declared<WireActionOptions>;
export type PageFunction<R = unknown> = (...args: any[]) => R;

export interface PerformOptions {
  operationId?: string;
  actionOptions?: ActionOptions;
  signal?: AbortSignal;
  timeoutMs?: number;
}

export interface PageWaitOptions extends CallOptions {
  waitTimeoutMs?: number;
}

export interface AgentObserveOptions extends CallOptions {
  cursor?: string;
  role?: string;
  name?: string;
  observationTimeoutMs?: number;
  maxControls?: number;
}

export interface AgentStepOptions extends AgentObserveOptions {
  destination?: Destination;
  expect?: Expectation;
  mode?: AgentMode;
  actionTimeoutMs?: number;
}

export interface ShadowClickResult { frameId: string; selector: string; x: number; y: number }
export interface TurnstileResult { clicks: number; elapsedMs: number }
export interface FetchOptions { method?: string; headers?: Record<string, string>; body?: string | null; redirect?: string }
export interface BoundedFetchOptions extends FetchOptions { maxBytes?: number }
export interface FetchResult { url: string; status: number; ok: boolean; headers: Record<string, string>; body: string }
export interface BoundedFetchResult {
  url: string; status: number; statusText: string; type: string; redirected: boolean;
  headers: Record<string, string>; body: string; truncated: boolean; bytesRead: number; bodyOmitted?: string | null;
}
export interface ViewportOptions { width: number; height: number; deviceScaleFactor?: number }
export interface ScreenshotOptions { format?: ScreenshotFormat | null; quality?: number | null; clip?: ScreenshotClip | null; fullPage?: boolean | null }
export type ScreenshotFileOptions = ScreenshotOptions & { path: string } & CallOptions;
export type ScreenshotFile = Artifact & { size: number };
export type RecordOptions = Pick<MethodParams['recording.start'], 'path' | 'fps' | 'maxWidth' | 'maxHeight'> & CallOptions & { recordingId?: string };

const recordingIdPattern = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i;
const actionOptionKeys = Object.keys(bundle.types.ActionOptions.properties);
const defaultActionTimeoutMs = bundle.types.ActionOptions.properties.timeout_ms.default;
const maximumWaitMs = bundle.methods['operations.wait'].params.properties.timeout_ms.maximum;
const maximumFetchBytes = bundle.methods.fetch.params.properties.max_body.maximum;
const defaultBoundedFetchBytes = 8192;

function locator(target: LocatorInput): LocatorSpec {
  return typeof target === 'string' ? { query: { kind: 'css', selector: target } } : target;
}

function comparable(name: string): string {
  return name.replace(/[^a-z0-9]/gi, '').toLowerCase();
}

function rejectUnknownActionOptions(options: object, operationId: string): void {
  for (const key of Object.keys(options)) {
    if (actionOptionKeys.includes(key)) continue;
    const intended = actionOptionKeys.find(name => comparable(name) === comparable(key));
    throw new ConfigurationError(
      `Unknown actionOptions key "${key}"${intended === undefined ? '.' : `; did you mean "${intended}"?`} Valid keys: ${actionOptionKeys.join(', ')}.`,
      { operationId, requestState: 'not_sent' },
    );
  }
}

function actionBudget(options: PerformOptions): { actionMs: number | undefined; localMs: number; settleMs: number } {
  const localMs = options.timeoutMs === undefined ? undefined : duration(options.timeoutMs, 0, 'timeoutMs');
  const requested = options.actionOptions?.timeout_ms;
  if (requested !== undefined && (typeof requested !== 'number' || !Number.isFinite(requested) || requested < 0)) {
    return { actionMs: undefined, localMs: localMs ?? defaultActionTimeoutMs + serverGraceMs, settleMs: 0 };
  }
  if (requested !== undefined && localMs !== undefined) return { actionMs: requested, localMs, settleMs: 0 };
  const actionMs = requested ?? (localMs === undefined ? defaultActionTimeoutMs : Math.ceil(localMs));
  return { actionMs, localMs: actionMs === 0 ? 0 : Math.min(maximumTimer, actionMs + serverGraceMs), settleMs: serverGraceMs / 2 };
}

function fetchParams(pageId: string, url: string, options: BoundedFetchOptions, maxBody: number): MethodParams['fetch'] {
  const { maxBytes: _maxBytes, ...request } = options;
  return { ...request, pageId, url, max_body: maxBody };
}

function captureParams(pageId: string, capture: ScreenshotOptions): MethodParams['screenshot'] {
  const { fullPage, ...rest } = capture;
  return { ...rest, pageId, ...(fullPage == null ? {} : { full_page: fullPage }) };
}

function localCallOptions(options: CallOptions): CallOptions {
  const local: CallOptions = {};
  if (options.signal !== undefined) local.signal = options.signal;
  if (options.timeoutMs !== undefined) local.timeoutMs = options.timeoutMs;
  if (options.operationId !== undefined) local.operationId = options.operationId;
  return local;
}

function functionSource(fn: string | PageFunction, method: string): string {
  if (typeof fn !== 'function') return fn;
  const source = fn.toString();
  if (/\{\s*\[native code\]\s*\}$/.test(source)) {
    throw new ConfigurationError(`${method} needs a function with source text; native and bound functions cannot be sent to the page`, { requestState: 'not_sent' });
  }
  return source;
}

export class Page implements InvokeClient {
  readonly id: string;
  private readonly client: InvokeClient;

  /**
   * Every operation the eoka CLI offers, bound to this page and generated from
   * the operation catalog: `page.ops.console()`, `page.ops.cookies()`,
   * `page.ops.networkRecordStart()`. Results are not yet typed.
   */
  readonly ops: CatalogOperations;

  constructor(client: InvokeClient, id: string) {
    this.client = client;
    this.id = id;
    this.ops = catalogOperations(this);
  }

  supports(method: string): boolean { return this.client.supports?.(method) === true; }
  hasCapability(name: string): boolean { return this.client.hasCapability?.(name) === true; }

  invoke<M extends MethodName>(method: M, params: MethodParams[M], options?: CallOptions): Promise<MethodResults[M]> {
    return this.client.invoke(method, params, options);
  }

  operation(id: string): Operation {
    return new Operation(this.client, id);
  }

  listOperations(params: MethodParams['operations.list'] = {}, options?: CallOptions): Promise<OperationRecord[]> {
    return this.invoke('operations.list', params, options);
  }

  async perform(action: Action, options: PerformOptions = {}): Promise<Operation> {
    const id = options.operationId ?? randomUUID();
    if (options.actionOptions != null && typeof options.actionOptions === 'object') rejectUnknownActionOptions(options.actionOptions, id);
    if ((action.kind === 'click' || action.kind === 'type' || action.kind === 'press_key' || action.kind === 'submit' || action.kind === 'drag' || action.kind === 'insert_text') && options.actionOptions?.expect == null) {
      throw new OperationError('expectation_required', 'This action requires actionOptions.expect before dispatch.', id, undefined, { requestState: 'not_sent' });
    }
    try {
      const params: MethodParams['actions.perform'] = { pageId: this.id, operation_id: id, action };
      if (options.actionOptions !== undefined) params.options = options.actionOptions;
      const callOptions: CallOptions = { operationId: id };
      if (options.signal !== undefined) callOptions.signal = options.signal;
      if (options.timeoutMs !== undefined) callOptions.timeoutMs = options.timeoutMs;
      const record = await this.invoke('actions.perform', params, callOptions);
      validateOperationRecord(record, id, this.id);
      return new Operation(this.client, id, record);
    } catch (error) {
      throw operationFailure(error, id);
    }
  }

  private async verified(action: Action, options: PerformOptions = {}): Promise<ActionReceipt> {
    const budget = actionBudget(options);
    const deadline = performance.now() + budget.localMs;
    const actionOptions = budget.actionMs === undefined ? options.actionOptions : { ...options.actionOptions, timeout_ms: budget.actionMs };
    const operation = await this.perform(action, { ...options, ...(actionOptions === undefined ? {} : { actionOptions }), timeoutMs: budget.localMs });
    let record = operation.record;
    const remaining = deadline - performance.now();
    if (remaining <= 0) {
      throw new OperationError('DeadlineExceeded', 'The local action deadline elapsed; recover the operation by ID to learn its outcome.', operation.id, record, { requestState: 'acknowledged' });
    }
    if (record?.state === 'running' || record === undefined) {
      const waitOptions: WaitOptions = { timeoutMs: remaining, waitTimeoutMs: Math.min(maximumWaitMs, Math.floor(Math.max(0, remaining - budget.settleMs))) };
      if (options.signal !== undefined) waitOptions.signal = options.signal;
      record = await operation.wait(waitOptions);
    }
    validateOperationRecord(record, operation.id, this.id);
    if (performance.now() >= deadline) {
      throw new OperationError('DeadlineExceeded', 'The local action deadline elapsed; recover the operation by ID to learn its outcome.', operation.id, record, { requestState: 'acknowledged' });
    }
    if (record.state !== 'finished' || record.outcome !== 'verified_complete' || record.receipt?.outcome !== 'verified_complete' || record.error_code != null) {
      const details: ErrorDetails = { requestState: 'acknowledged' };
      if (record.error != null) details.cause = new EokaError(record.error.code, record.error.message, { operationId: operation.id, data: record.error });
      throw new OperationError(record.error_code ?? (record.state === 'running' ? 'OperationRunning' : record.outcome),
        actionFailureMessage(action, actionOptions?.expect, budget.actionMs, record), operation.id, record, details);
    }
    return record.receipt;
  }

  goto(url: string, options?: PerformOptions): Promise<ActionReceipt> {
    return this.verified({ kind: 'navigate', url }, options);
  }

  fill(target: LocatorInput, value: string, options: PerformOptions & { inputMode?: InputMode } = {}): Promise<ActionReceipt> {
    return this.verified({ kind: 'fill', target: locator(target), value, input_mode: options.inputMode ?? 'sequential' }, options);
  }

  select(target: LocatorInput, value: string | string[], options?: PerformOptions): Promise<ActionReceipt> {
    return this.verified({ kind: 'select', target: locator(target), values: typeof value === 'string' ? [value] : value }, options);
  }

  hover(target: LocatorInput, options?: PerformOptions): Promise<ActionReceipt> {
    return this.verified({ kind: 'hover', target: locator(target) }, options);
  }

  click(target: LocatorInput, options?: PerformOptions): Promise<ActionReceipt> {
    return this.verified({ kind: 'click', target: locator(target) }, options);
  }

  type(target: LocatorInput, text: string, options: PerformOptions & { inputMode?: InputMode } = {}): Promise<ActionReceipt> {
    return this.verified({ kind: 'type', target: locator(target), text, input_mode: options.inputMode ?? 'sequential' }, options);
  }

  pressKey(key: string, options?: PerformOptions): Promise<ActionReceipt> {
    return this.verified({ kind: 'press_key', key }, options);
  }

  submit(target: LocatorInput, options?: PerformOptions): Promise<ActionReceipt> {
    return this.verified({ kind: 'submit', target: locator(target) }, options);
  }

  drag(target: LocatorInput, dx: number, dy: number, options?: PerformOptions): Promise<ActionReceipt> {
    return this.verified({ kind: 'drag', target: locator(target), dx, dy }, options);
  }

  async text(options?: CallOptions): Promise<string> {
    return this.invoke('text', { pageId: this.id }, options);
  }

  async content(options?: CallOptions): Promise<string> {
    return this.invoke('content', { pageId: this.id }, options);
  }

  async title(options?: CallOptions): Promise<string> {
    return (await this.invoke('info', { pageId: this.id }, options)).title;
  }

  async url(options?: CallOptions): Promise<string> {
    return (await this.invoke('info', { pageId: this.id }, options)).url;
  }

  async getText(selector: string, options?: CallOptions): Promise<string> {
    return this.invoke('get_text', { pageId: this.id, selector }, options);
  }

  async getAttribute(selector: string, name: string, options?: CallOptions): Promise<string | null> {
    return (await this.invoke('get_attribute', { pageId: this.id, selector, name }, options)).value ?? null;
  }

  async exists(selector: string, options?: CallOptions): Promise<boolean> {
    return (await this.invoke('exists', { pageId: this.id, selector }, options)).exists;
  }

  async waitFor(selector: string, options: PageWaitOptions = {}): Promise<void> {
    const { waitTimeoutMs = 30_000, ...localOptions } = options;
    await this.invoke('wait', { pageId: this.id, selector, state: 'attached', timeout: waitTimeoutMs }, localOptions);
  }

  async waitForVisible(selector: string, options: PageWaitOptions = {}): Promise<void> {
    const { waitTimeoutMs = 30_000, ...localOptions } = options;
    await this.invoke('wait', { pageId: this.id, selector, state: 'visible', timeout: waitTimeoutMs }, localOptions);
  }

  async waitForText(text: string, options: PageWaitOptions = {}): Promise<void> {
    const { waitTimeoutMs = 30_000, ...localOptions } = options;
    await this.invoke('wait', { pageId: this.id, text, timeout: waitTimeoutMs }, localOptions);
  }

  evaluate<R>(fn: PageFunction<R>, args?: unknown[], options?: CallOptions): Promise<Awaited<R>>;
  evaluate(fn: string, args?: unknown[], options?: CallOptions): Promise<unknown>;
  async evaluate(fn: string | PageFunction, args: unknown[] = [], options?: CallOptions): Promise<unknown> {
    return this.invoke('eval', { pageId: this.id, code: functionSource(fn, 'page.evaluate'), function: true, arguments: args }, options);
  }

  async evaluateRaw(js: string, options?: CallOptions): Promise<unknown> {
    return this.invoke('eval', { pageId: this.id, code: js, raw: true, json: true }, options);
  }

  async execute(fn: string | PageFunction, args: unknown[] = [], options?: CallOptions): Promise<void> {
    await this.invoke('exec', { pageId: this.id, code: functionSource(fn, 'page.execute'), function: true, arguments: args }, options);
  }

  async executeRaw(js: string, options?: CallOptions): Promise<void> {
    await this.invoke('exec', { pageId: this.id, code: js, raw: true }, options);
  }

  async frames(options?: CallOptions): Promise<MethodResults['frames']> {
    return this.invoke('frames', { pageId: this.id }, options);
  }

  async clickShadow(
    selector: string,
    options: { frameId?: string; resolveTimeoutMs?: number } & CallOptions = {},
  ): Promise<ShadowClickResult> {
    const { frameId, resolveTimeoutMs, ...callOptions } = options;
    if (callOptions.timeoutMs === undefined && resolveTimeoutMs !== undefined) {
      callOptions.timeoutMs = resolveTimeoutMs + serverGraceMs;
    }
    const click = await this.invoke('click_shadow', {
      pageId: this.id,
      selector,
      ...(frameId === undefined ? {} : { frame_id: frameId }),
      ...(resolveTimeoutMs === undefined ? {} : { timeout_ms: resolveTimeoutMs }),
    }, callOptions);
    return { frameId: click.frame_id, selector: click.selector, x: click.x, y: click.y };
  }

  async solveTurnstile(options: { timeoutMs?: number } & CallOptions = {}): Promise<TurnstileResult> {
    const { timeoutMs = 30_000, ...callOptions } = options;
    const solved = await this.invoke('captcha.turnstile', { pageId: this.id, timeout_ms: timeoutMs }, { ...callOptions, timeoutMs: timeoutMs + serverGraceMs });
    return { clicks: solved.clicks, elapsedMs: solved.elapsed_ms };
  }

  async captureState(options?: CallOptions): Promise<BrowserState> {
    return (await this.invoke('page.capture_state', { pageId: this.id }, options)).state;
  }

  async restoreState(state: BrowserState, options?: CallOptions): Promise<void> {
    await this.invoke('page.restore_state', { pageId: this.id, state }, options);
  }

  async fetch(url: string, fetchOptions: FetchOptions = {}, options?: CallOptions): Promise<FetchResult> {
    const response = await this.invoke('fetch', fetchParams(this.id, url, fetchOptions, maximumFetchBytes), options) as BoundedFetchResult;
    if (response.truncated || response.bodyOmitted === 'binary') {
      throw new EokaError('FetchBodyUnavailable', `The response body ${response.truncated ? `exceeds ${maximumFetchBytes} bytes` : 'is binary'}; use fetchBounded to inspect it.`, { requestState: 'acknowledged', data: response });
    }
    const { url: finalUrl, status, headers, body } = response;
    return { url: finalUrl, status, ok: status >= 200 && status < 300, headers, body };
  }

  async fetchBounded(url: string, fetchOptions: BoundedFetchOptions = {}, options?: CallOptions): Promise<BoundedFetchResult> {
    return await this.invoke('fetch', fetchParams(this.id, url, fetchOptions, fetchOptions.maxBytes ?? defaultBoundedFetchBytes), options) as BoundedFetchResult;
  }

  async screenshot(options: ScreenshotOptions & CallOptions = {}): Promise<Buffer> {
    requireCaptureCapability(this.client, 'screenshot', 'screenshotOptions');
    const { signal, timeoutMs, operationId, ...capture } = options;
    const shot = await this.invoke('screenshot', { ...captureParams(this.id, capture), inline: true }, localCallOptions(options)) as { dataBase64: string };
    return Buffer.from(shot.dataBase64, 'base64');
  }

  async screenshotToFile(options: ScreenshotFileOptions): Promise<ScreenshotFile> {
    requireCaptureCapability(this.client, 'screenshot', 'screenshotFile');
    const { signal, timeoutMs, operationId, path, ...capture } = options;
    return await this.invoke('screenshot', { ...captureParams(this.id, capture), output: resolve(path) }, localCallOptions(options)) as ScreenshotFile;
  }

  async record(options: RecordOptions): Promise<Recording> {
    const id = options.recordingId ?? randomUUID();
    let path: string | undefined;
    try {
      path = resolve(options.path);
      requireCaptureCapability(this.client, 'recording.start', 'nativeRecording');
      if (!recordingIdPattern.test(id)) {
        throw new RecordingError('Configuration', 'recordingId must be a UUID.', id, undefined, { requestState: 'not_sent' }, path);
      }
      const { signal, timeoutMs, operationId, recordingId, path: outputPath, ...nativeOptions } = options;
      const localOptions: CallOptions = {};
      if (signal !== undefined) localOptions.signal = signal;
      if (timeoutMs !== undefined) localOptions.timeoutMs = timeoutMs;
      if (operationId !== undefined) localOptions.operationId = operationId;
      const record = await this.invoke('recording.start', { ...nativeOptions, pageId: this.id, recordingId: id, path }, localOptions);
      validateRecordingRecord(record, id, this.id, path);
      if (record.state === 'failed' || record.state === 'cancelled') {
        throw new RecordingError(record.failure?.code ?? 'RecordingInvalidState', record.failure?.message ?? `Recording ${id} is ${record.state}.`, id, record, { requestState: 'acknowledged' });
      }
      return new Recording(this.client, id, record);
    } catch (error) {
      throw recordingFailure(error, id, undefined, path, this.id);
    }
  }

  async setViewport(viewport: ViewportOptions, options?: CallOptions): Promise<void> {
    const { width, height, deviceScaleFactor = 1 } = viewport;
    await this.invoke('emulate', { pageId: this.id, width, height, dpr: deviceScaleFactor, desktop: true }, options);
  }

  async resetViewport(options?: CallOptions): Promise<void> {
    await this.invoke('emulate', { pageId: this.id, reset: true }, options);
  }

  observe(options: AgentObserveOptions = {}): Promise<AgentObservation> {
    const { observationTimeoutMs, maxControls, cursor, role, name, ...localOptions } = options;
    const params: MethodParams['agent.observe'] = { pageId: this.id };
    if (cursor !== undefined) params.cursor = cursor;
    if (role !== undefined) params.role = role;
    if (name !== undefined) params.name = name;
    if (observationTimeoutMs !== undefined) params.timeout_ms = observationTimeoutMs;
    if (maxControls !== undefined) params.max_controls = maxControls;
    return this.invoke('agent.observe', params, localOptions);
  }

  async step(action: AgentAction, options: AgentStepOptions = {}): Promise<AgentStepResult> {
    const { operationId = randomUUID(), observationTimeoutMs, maxControls, actionTimeoutMs, expect, mode, destination, ...localOptions } = options;
    try {
      const params: MethodParams['agent.step'] = { pageId: this.id, action, operation_id: operationId };
      if (expect !== undefined) params.expect = expect;
      if (mode !== undefined) params.mode = mode;
      if (destination !== undefined) params.destination = destination;
      if (actionTimeoutMs !== undefined) params.action_timeout_ms = actionTimeoutMs;
      if (observationTimeoutMs !== undefined) params.timeout_ms = observationTimeoutMs;
      if (maxControls !== undefined) params.max_controls = maxControls;
      const callOptions: CallOptions = localOptions;
      callOptions.operationId = operationId;
      const result = await this.invoke('agent.step', params, callOptions);
      if (result.operation_id !== operationId) {
        throw new OperationError('Protocol', `Received an agent result for a different operation than ${operationId}.`, operationId, result.operation ?? undefined, { requestState: 'acknowledged' });
      }
      if (result.operation != null) validateOperationRecord(result.operation, operationId, this.id);
      return result;
    } catch (error) {
      throw operationFailure(error, operationId);
    }
  }

  async close(options?: CallOptions): Promise<void> {
    const closed = await this.invoke('browser.close_tab', { pageId: this.id }, options);
    if (closed.cleanupError != null) {
      throw new EokaError('CleanupFailed', `The tab closed but cleanup failed: ${closed.cleanupError}`, { requestState: 'acknowledged', data: closed });
    }
  }
}

traceCallSites(Page.prototype);
