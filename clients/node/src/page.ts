import { Buffer } from 'node:buffer';
import { randomUUID } from 'node:crypto';
import { resolve } from 'node:path';
import type { CallOptions, InvokeClient } from './client.js';
import type {
  Action, ActionOptions, ActionReceipt, AgentAction, AgentMode, AgentObservation,
  AgentStepResult, BrowserState, Expectation, LocatorSpec, MethodName,
  MethodParams, MethodResults, OperationRecord,
} from './generated/protocol.js';
import { Operation, OperationError, operationFailure, validateOperationRecord, type WaitOptions } from './operations.js';
import { Recording, RecordingError, recordingFailure, requireCaptureCapability, validateRecordingRecord } from './recording.js';

export type LocatorInput = string | LocatorSpec;

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
  observationTimeoutMs?: number;
  maxControls?: number;
}

export interface AgentStepOptions extends AgentObserveOptions {
  expect?: Expectation;
  mode?: AgentMode;
  actionTimeoutMs?: number;
}

export type FetchOptions = Pick<MethodParams['page.fetch'], 'method' | 'headers' | 'body' | 'redirect'>;
export type BoundedFetchOptions = Pick<MethodParams['page.fetch_bounded'], 'method' | 'headers' | 'body' | 'redirect' | 'maxBytes'>;
export type ViewportOptions = Pick<MethodParams['page.set_viewport'], 'width' | 'height' | 'deviceScaleFactor'>;
export type ScreenshotOptions = Pick<MethodParams['page.screenshot'], 'format' | 'quality' | 'clip' | 'fullPage'>;
export type ScreenshotFileOptions = ScreenshotOptions & Pick<MethodParams['page.screenshot_to_file'], 'path'> & CallOptions;
export type RecordOptions = Pick<MethodParams['recording.start'], 'path' | 'fps' | 'maxWidth' | 'maxHeight'> & CallOptions & { recordingId?: string };

const recordingIdPattern = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i;

function locator(target: LocatorInput): LocatorSpec {
  return typeof target === 'string' ? { query: { kind: 'css', selector: target } } : target;
}

export class Page implements InvokeClient {
  readonly id: string;
  private readonly client: InvokeClient;

  constructor(client: InvokeClient, id: string) {
    this.client = client;
    this.id = id;
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
    const timeoutMs = options.timeoutMs ?? 30_000;
    const deadline = performance.now() + timeoutMs;
    const operation = await this.perform(action, { ...options, timeoutMs });
    let record = operation.record;
    const remaining = deadline - performance.now();
    if (remaining <= 0) {
      throw new OperationError('DeadlineExceeded', 'The local action deadline elapsed; recover the operation by ID to learn its outcome.', operation.id, record, { requestState: 'acknowledged' });
    }
    if (record?.state === 'running' || record === undefined) {
      const waitOptions: WaitOptions = { timeoutMs: remaining, waitTimeoutMs: Math.floor(remaining) };
      if (options.signal !== undefined) waitOptions.signal = options.signal;
      record = await operation.wait(waitOptions);
    }
    validateOperationRecord(record, operation.id, this.id);
    if (performance.now() >= deadline) {
      throw new OperationError('DeadlineExceeded', 'The local action deadline elapsed; recover the operation by ID to learn its outcome.', operation.id, record, { requestState: 'acknowledged' });
    }
    if (record.state !== 'finished' || record.outcome !== 'verified_complete' || record.receipt?.outcome !== 'verified_complete' || record.error_code != null) {
      throw new OperationError(record.error_code ?? (record.state === 'running' ? 'OperationRunning' : record.outcome),
        `Operation ${operation.id} is ${record.state} with outcome ${record.outcome}; verified completion was not established.`,
        operation.id, record, { requestState: 'acknowledged' });
    }
    return record.receipt;
  }

  goto(url: string, options?: PerformOptions): Promise<ActionReceipt> {
    return this.verified({ kind: 'navigate', url }, options);
  }

  fill(target: LocatorInput, value: string, options?: PerformOptions): Promise<ActionReceipt> {
    return this.verified({ kind: 'fill', target: locator(target), value }, options);
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

  type(target: LocatorInput, text: string, options?: PerformOptions): Promise<ActionReceipt> {
    return this.verified({ kind: 'type', target: locator(target), text }, options);
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
    return (await this.invoke('page.text', { pageId: this.id }, options)).text;
  }

  async content(options?: CallOptions): Promise<string> {
    return (await this.invoke('page.content', { pageId: this.id }, options)).html;
  }

  async title(options?: CallOptions): Promise<string> {
    return (await this.invoke('page.title', { pageId: this.id }, options)).title;
  }

  async url(options?: CallOptions): Promise<string> {
    return (await this.invoke('page.url', { pageId: this.id }, options)).url;
  }

  async getText(selector: string, options?: CallOptions): Promise<string> {
    return (await this.invoke('page.get_text', { pageId: this.id, selector }, options)).text;
  }

  async getAttribute(selector: string, name: string, options?: CallOptions): Promise<string | null> {
    return (await this.invoke('page.get_attribute', { pageId: this.id, selector, name }, options)).value ?? null;
  }

  async exists(selector: string, options?: CallOptions): Promise<boolean> {
    return (await this.invoke('page.exists', { pageId: this.id, selector }, options)).exists;
  }

  async waitFor(selector: string, options: PageWaitOptions = {}): Promise<void> {
    const { waitTimeoutMs = 30_000, ...localOptions } = options;
    await this.invoke('page.wait_for', { pageId: this.id, selector, timeoutMs: waitTimeoutMs }, localOptions);
  }

  async waitForVisible(selector: string, options: PageWaitOptions = {}): Promise<void> {
    const { waitTimeoutMs = 30_000, ...localOptions } = options;
    await this.invoke('page.wait_for_visible', { pageId: this.id, selector, timeoutMs: waitTimeoutMs }, localOptions);
  }

  async waitForText(text: string, options: PageWaitOptions = {}): Promise<void> {
    const { waitTimeoutMs = 30_000, ...localOptions } = options;
    await this.invoke('page.wait_for_text', { pageId: this.id, text, timeoutMs: waitTimeoutMs }, localOptions);
  }

  async evaluate(js: string, options?: CallOptions): Promise<unknown> {
    return (await this.invoke('page.evaluate', { pageId: this.id, js }, options)).result;
  }

  async execute(js: string, options?: CallOptions): Promise<void> {
    await this.invoke('page.execute', { pageId: this.id, js }, options);
  }

  /** List every frame in the page, including cross-origin OOPIFs. */
  async frames(options?: CallOptions): Promise<MethodResults['page.frames']> {
    return this.invoke('page.frames', { pageId: this.id }, options);
  }

  /**
   * Click an element inside a closed shadow root in the top frame or a child
   * frame. `frameId` comes from `frames()`. The selector is searched through
   * the frame's light DOM and every shadow root; the node's center is mapped
   * to viewport coordinates and clicked with human-like movement.
   */
  async clickShadow(
    selector: string,
    options: { frameId?: string; resolveTimeoutMs?: number } & CallOptions = {},
  ): Promise<MethodResults['page.click_shadow']> {
    const { frameId, resolveTimeoutMs, ...callOptions } = options;
    // `CallOptions.timeoutMs` is the RPC deadline; `resolveTimeoutMs` is the
    // server-side resolution deadline. Derive an RPC deadline that cannot
    // outlive a long resolution when the caller did not set one explicitly.
    if (callOptions.timeoutMs === undefined && resolveTimeoutMs !== undefined) {
      callOptions.timeoutMs = resolveTimeoutMs + 10_000;
    }
    return this.invoke('page.click_shadow', {
      pageId: this.id,
      selector,
      ...(frameId === undefined ? {} : { frameId }),
      ...(resolveTimeoutMs === undefined ? {} : { timeoutMs: resolveTimeoutMs }),
    }, callOptions);
  }

  /**
   * Best-effort helper for interactive Cloudflare challenge pages: the server
   * clicks the Turnstile checkbox through its closed shadow root until the page
   * leaves the challenge. Resolves with zero clicks on a normal page.
   */
  solveTurnstile(options: { timeoutMs?: number } & CallOptions = {}): Promise<MethodResults['page.solve_turnstile']> {
    const { timeoutMs = 30_000, ...callOptions } = options;
    // The local deadline outlasts the server's solve deadline so its error wins.
    return this.invoke('page.solve_turnstile', { pageId: this.id, timeoutMs }, { ...callOptions, timeoutMs: timeoutMs + 10_000 });
  }

  async captureState(options?: CallOptions): Promise<BrowserState> {
    return (await this.invoke('page.capture_state', { pageId: this.id }, options)).state;
  }

  async restoreState(state: BrowserState, options?: CallOptions): Promise<void> {
    await this.invoke('page.restore_state', { pageId: this.id, state }, options);
  }

  fetch(url: string, fetchOptions: FetchOptions = {}, options?: CallOptions): Promise<MethodResults['page.fetch']> {
    return this.invoke('page.fetch', { ...fetchOptions, pageId: this.id, url }, options);
  }

  fetchBounded(url: string, fetchOptions: BoundedFetchOptions = {}, options?: CallOptions): Promise<MethodResults['page.fetch_bounded']> {
    return this.invoke('page.fetch_bounded', { ...fetchOptions, pageId: this.id, url }, options);
  }

  async screenshot(options: ScreenshotOptions & CallOptions = {}): Promise<Buffer> {
    requireCaptureCapability(this.client, 'page.screenshot', 'screenshotOptions');
    const { signal, timeoutMs, operationId, ...capture } = options;
    const localOptions: CallOptions = {};
    if (signal !== undefined) localOptions.signal = signal;
    if (timeoutMs !== undefined) localOptions.timeoutMs = timeoutMs;
    if (operationId !== undefined) localOptions.operationId = operationId;
    return Buffer.from((await this.invoke('page.screenshot', { ...capture, pageId: this.id }, localOptions)).dataBase64, 'base64');
  }

  async screenshotToFile(options: ScreenshotFileOptions): Promise<MethodResults['page.screenshot_to_file']> {
    requireCaptureCapability(this.client, 'page.screenshot_to_file', 'screenshotFile');
    const { signal, timeoutMs, operationId, path, ...capture } = options;
    const localOptions: CallOptions = {};
    if (signal !== undefined) localOptions.signal = signal;
    if (timeoutMs !== undefined) localOptions.timeoutMs = timeoutMs;
    if (operationId !== undefined) localOptions.operationId = operationId;
    return this.invoke('page.screenshot_to_file', { ...capture, pageId: this.id, path: resolve(path) }, localOptions);
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
    await this.invoke('page.set_viewport', { ...viewport, pageId: this.id }, options);
  }

  async resetViewport(options?: CallOptions): Promise<void> {
    await this.invoke('page.reset_viewport', { pageId: this.id }, options);
  }

  observe(options: AgentObserveOptions = {}): Promise<AgentObservation> {
    const { observationTimeoutMs, maxControls, ...localOptions } = options;
    const params: MethodParams['agent.observe'] = { pageId: this.id };
    if (observationTimeoutMs !== undefined) params.timeout_ms = observationTimeoutMs;
    if (maxControls !== undefined) params.max_controls = maxControls;
    return this.invoke('agent.observe', params, localOptions);
  }

  async step(action: AgentAction, options: AgentStepOptions = {}): Promise<AgentStepResult> {
    const { operationId = randomUUID(), observationTimeoutMs, maxControls, actionTimeoutMs, expect, mode, ...localOptions } = options;
    try {
      const params: MethodParams['agent.step'] = { pageId: this.id, action, operation_id: operationId };
      if (expect !== undefined) params.expect = expect;
      if (mode !== undefined) params.mode = mode;
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
    await this.invoke('page.close', { pageId: this.id }, options);
  }
}
