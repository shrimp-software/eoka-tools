import { spawn, type ChildProcessWithoutNullStreams } from 'node:child_process';
import { TextDecoder } from 'node:util';
import { join } from 'node:path';
import { Ajv2020 } from 'ajv/dist/2020.js';
import type { ValidateFunction } from 'ajv';
import bundle from './generated/schema-bundle.json' with { type: 'json' };
import { protocolVersion, type ActionReceipt, type MethodName, type MethodParams, type MethodResults, type OperationRecord, type ServerError } from './generated/protocol.js';
import type { CallOptions, InvokeClient } from './client.js';
import { ConfigurationError, EokaError, ProtocolError, ServerOperationError, TransportError, type ErrorDetails } from './errors.js';

export interface SidecarStartOptions {
  serverPath: string;
  env?: NodeJS.ProcessEnv;
  timeoutMs?: number;
  shutdownTimeoutMs?: number;
  signal?: AbortSignal;
  redactionValues?: readonly string[];
  onTransport?: (transport: SidecarTransport) => void;
}

export interface TrackedInvocation<M extends MethodName> {
  result: Promise<MethodResults[M]>;
  completion: Promise<MethodResults[M]>;
}

type Completion = { resolve: (value: unknown) => void; reject: (error: unknown) => void };

type Pending = {
  id: number;
  method: MethodName;
  operationId: string | undefined;
  sent: boolean;
  frame: Buffer | undefined;
  resolve: ((value: unknown) => void) | undefined;
  reject: ((error: EokaError) => void) | undefined;
  completion?: Completion | undefined;
  signal: AbortSignal | undefined;
  abort: (() => void) | undefined;
  timer: NodeJS.Timeout | undefined;
};

type Contract = { params: object; result: { title?: string } };
const contracts = bundle.methods as Record<MethodName, Contract>;
let ajv: Ajv2020 | undefined;
const validators = new Map<string, ValidateFunction>();
const localMessageLimit = 32 * 1024 * 1024;
const localPendingLimit = 128;
const stderrLimit = 16 * 1024;
const maximumTimer = 2_147_483_647;

function duration(value: number | undefined, fallback: number, name: string): number {
  const result = value ?? fallback;
  if (!Number.isFinite(result) || result < 0 || result > maximumTimer) {
    throw new ConfigurationError(`${name} must be finite and between 0 and ${maximumTimer}`, { requestState: 'not_sent' });
  }
  return result;
}

function secretMatcher(values: readonly string[]): RegExp | undefined {
  const alternatives = new Set<string>();
  for (const value of values) {
    if (!value) continue;
    alternatives.add(value);
    alternatives.add(encodeURIComponent(value));
    alternatives.add(JSON.stringify(value).slice(1, -1));
  }
  if (alternatives.size === 0) return undefined;
  const escaped = [...alternatives].sort((left, right) => right.length - left.length).map(value => value.replace(/[.*+?^${}()|[\]\\]/g, '\\$&'));
  return new RegExp(escaped.join('|'), 'g');
}

function redactLine(line: string, secrets: RegExp | undefined): string {
  let result = line.replace(/\b[a-z][a-z\d+.-]*:\/\/[^\s<>"']+/gi, text => {
    if (secrets && text.replace(secrets, '[REDACTED]') !== text) return '[REDACTED URL]';
    try {
      const url = new URL(text);
      const path = url.pathname.replace(/\/devtools\/(?:browser|page)\/.*/gi, '/devtools/[REDACTED]');
      return `${url.protocol}//${url.host}${path}${url.search ? '?[REDACTED]' : ''}${url.hash ? '#[REDACTED]' : ''}`;
    } catch {
      return '[REDACTED URL]';
    }
  });
  if (secrets) result = result.replace(secrets, '[REDACTED]');
  result = result.replace(/(\b["']?(?:[a-z\d]+[-_])*(?:proxy[-_]?authorization|authorization|authentication|auth|set[-_]?cookie|cookie|(?:access[-_]?|refresh[-_]?)?token|password|passwd|secret|api[-_]?key)["']?\s*[:=]\s*).*/gi, '$1[REDACTED]');
  result = result.replace(/(\b(?:Bearer|Basic)\s+).*/gi, '$1[REDACTED]');
  return result.replace(/(\B--(?:password|passwd|token|api[-_]?key)\s+).*/gi, '$1[REDACTED]');
}

function object(value: unknown): value is Record<string, unknown> {
  return value !== null && typeof value === 'object' && !Array.isArray(value);
}

function receiptEvidence(receipt: ActionReceipt): void {
  if (receipt.outcome === 'verified_complete' && (!receipt.verification || (receipt.dispatch !== 'acknowledged' && !receipt.already_satisfied))) {
    throw new ProtocolError('Verified receipt is missing completion or dispatch evidence');
  }
  if (receipt.outcome === 'not_dispatched' && receipt.dispatch !== 'not_sent') {
    throw new ProtocolError('Receipt contradicts its dispatch state');
  }
}

function operationEvidence(record: OperationRecord, expectedId?: string): void {
  if (expectedId !== undefined && record.id !== expectedId) throw new ProtocolError('Operation response has a different operation ID');
  if (record.receipt) {
    receiptEvidence(record.receipt);
    if (record.receipt.outcome !== record.outcome || record.receipt.action !== record.action || record.receipt.target_id !== record.target_id) {
      throw new ProtocolError('Operation record contradicts its receipt');
    }
  }
  if (record.outcome === 'verified_complete' && (record.state !== 'finished' || !record.receipt || record.finished_at_ms == null)) {
    throw new ProtocolError('Verified operation is missing terminal receipt evidence');
  }
  if (record.state === 'running' && (record.receipt != null || record.finished_at_ms != null)) {
    throw new ProtocolError('Running operation contains terminal evidence');
  }
}

export class SidecarTransport implements InvokeClient {
  private readonly child: ChildProcessWithoutNullStreams;
  private readonly shutdownTimeoutMs: number;
  private readonly pending = new Map<number, Pending>();
  private readonly decoder = new TextDecoder('utf-8', { fatal: true, ignoreBOM: true });
  private greeting: MethodResults['server.hello'] | undefined;
  private supported = new Set<string>();
  private nextId = 1;
  private messageLimit = localMessageLimit;
  private pendingLimit = localPendingLimit;
  private frame = Buffer.alloc(0);
  private frameLength = 0;
  private queuedBytes = 0;
  private blocked = false;
  private closing = false;
  private quiescing = false;
  private finished = false;
  private exited = false;
  private stderrBytes = Buffer.alloc(0);
  private stderrClipped = false;
  private finalStderr: string | undefined;
  private readonly secrets: RegExp | undefined;
  private escalating = false;
  private failure: EokaError | undefined;
  private cleanupError: EokaError | undefined;
  private closeCompletion: PromiseWithResolvers<void> | undefined;
  private teardownTimer: NodeJS.Timeout | undefined;
  private shutdownId: number | undefined;
  private readonly closed = Promise.withResolvers<void>();
  private terminationConfirmed = false;
  private shutdownAcknowledged = false;
  private childClosed = false;
  private windowsTreeTerminated = false;

  private constructor(child: ChildProcessWithoutNullStreams, shutdownTimeoutMs: number, secrets: RegExp | undefined) {
    void this.closed.promise.catch(() => {});
    this.child = child;
    this.shutdownTimeoutMs = shutdownTimeoutMs;
    this.secrets = secrets;
    child.stdout.on('data', this.onData);
    child.stdout.on('end', this.onEnd);
    child.stdout.on('error', this.onStreamError);
    child.stderr.on('data', this.onStderr);
    child.stderr.on('error', this.onStreamError);
    child.stdin.on('error', this.onStreamError);
    child.stdin.on('drain', this.onDrain);
    child.on('error', this.onProcessError);
    child.on('exit', this.onExit);
    child.on('close', this.onClose);
  }

  static async start(options: SidecarStartOptions): Promise<SidecarTransport> {
    const timeoutMs = duration(options.timeoutMs, 30_000, 'timeoutMs');
    const shutdownTimeoutMs = duration(options.shutdownTimeoutMs, 45_000, 'shutdownTimeoutMs');
    const secrets = secretMatcher(options.redactionValues ?? []);
    if (!options.serverPath) throw new ConfigurationError('An explicit serverPath is required', { requestState: 'not_sent' });
    if (options.signal?.aborted) throw new TransportError('Aborted', 'Sidecar startup was aborted before spawning', { requestState: 'not_sent', cause: options.signal.reason });
    let child: ChildProcessWithoutNullStreams;
    try {
      child = spawn(options.serverPath, [], { shell: false, stdio: 'pipe', detached: process.platform !== 'win32', ...(options.env === undefined ? {} : { env: options.env }) });
    } catch (cause) {
      throw new TransportError('Spawn', 'Could not spawn the sidecar', { cause, requestState: 'not_sent' });
    }
    const transport = new SidecarTransport(child, shutdownTimeoutMs, secrets);
    try {
      options.onTransport?.(transport);
      const hello = await transport.request('server.hello', { protocol: protocolVersion }, { timeoutMs, ...(options.signal === undefined ? {} : { signal: options.signal }) });
      if (transport.failure) throw transport.failure;
      if (transport.closing || transport.finished) throw new TransportError('Closed', 'Sidecar closed during handshake');
      if (hello.protocol.major !== protocolVersion.major || hello.protocol.minor < protocolVersion.minor) {
        throw new ProtocolError('Sidecar protocol is incompatible with this SDK');
      }
      const shutdownBytes = Buffer.byteLength(JSON.stringify({ id: Number.MAX_SAFE_INTEGER, method: 'server.shutdown', params: {} }));
      if (hello.limits.maxInFlight < 1 || hello.limits.maxMessageBytes < shutdownBytes || !hello.methods.includes('server.shutdown')) {
        throw new ProtocolError('Sidecar advertised unusable transport limits or shutdown support');
      }
      transport.greeting = hello;
      transport.supported = new Set(hello.methods);
      transport.messageLimit = Math.min(localMessageLimit, hello.limits.maxMessageBytes);
      transport.pendingLimit = Math.min(localPendingLimit, hello.limits.maxInFlight);
      if (transport.frameLength > transport.messageLimit) throw new ProtocolError('Buffered peer frame exceeds negotiated limit');
      return transport;
    } catch (error) {
      if (error instanceof EokaError) transport.failure ??= error;
      try {
        await transport.close();
      } catch (cleanup) {
        throw new TransportError('StartupCleanup', 'Sidecar startup failed and cleanup did not complete gracefully', { cause: new AggregateError([error, cleanup]), stderr: transport.stderr });
      }
      throw error;
    }
  }

  get hello(): MethodResults['server.hello'] {
    if (!this.greeting) throw new ProtocolError('Sidecar handshake has not completed');
    return this.greeting;
  }

  get pid(): number | undefined {
    return this.child.pid;
  }
  private get stderr(): string {
    if (this.finalStderr !== undefined) return this.finalStderr;
    const text = this.stderrBytes.toString('utf8');
    const first = this.stderrClipped ? text.indexOf('\n') + 1 : 0;
    const last = text.lastIndexOf('\n');
    if (last < first || (this.stderrClipped && first === 0)) return text ? '[REDACTED partial stderr omitted]' : '';
    const lines = text.slice(first, last).split('\n');
    const safe: string[] = [];
    let bytes = 0;
    for (const line of lines) {
      let redacted = redactLine(line, this.secrets);
      let length = Buffer.byteLength(redacted) + 1;
      if (length > stderrLimit) {
        redacted = '[REDACTED oversized stderr line]';
        length = Buffer.byteLength(redacted) + 1;
      }
      while (bytes + length > stderrLimit && safe.length > 0) bytes -= Buffer.byteLength(safe.shift()!) + 1;
      safe.push(redacted);
      bytes += length;
    }
    return safe.join('\n');
  }

  supports(method: string): boolean { return this.supported.has(method); }
  hasCapability(name: string): boolean { return this.greeting?.capabilities.includes(name) === true; }

  invoke<M extends MethodName>(method: M, params: MethodParams[M], options: CallOptions = {}): Promise<MethodResults[M]> {
    return this.request(method, params, options);
  }

  invokeTracked<M extends MethodName>(method: M, params: MethodParams[M], options: CallOptions = {}): TrackedInvocation<M> {
    const { promise: completion, resolve, reject } = Promise.withResolvers<MethodResults[M]>();
    const callbacks: Completion = { resolve: value => resolve(value as MethodResults[M]), reject };
    void completion.catch(() => {});
    const result = this.request(method, params, options, callbacks);
    return { result, completion };
  }

  quiesce(): void {
    this.quiescing = true;
    this.rejectPending(new TransportError('Closed', 'Sidecar transport is being disposed'));
  }

  invokeCleanup(method: 'browser.close_tab', params: MethodParams['browser.close_tab'], options: { timeoutMs?: number } = {}): Promise<MethodResults['browser.close_tab']> {
    if (method !== 'browser.close_tab') return Promise.reject(new ConfigurationError('Only exact-tab cleanup is permitted during disposal', { requestState: 'not_sent' }));
    return this.request(method, params, options, undefined, true);
  }

  private validate(key: string, schema: object, value: unknown): void {
    const compiler = ajv ??= new Ajv2020({ strict: false, validateFormats: false });
    let validator = validators.get(key);
    if (!validator) {
      validator = compiler.compile(schema);
      validators.set(key, validator);
    }
    if (!validator(value)) throw new ProtocolError(`Invalid ${key}: ${compiler.errorsText(validator.errors)}`);
  }

  private request<M extends MethodName>(method: M, params: MethodParams[M], options: CallOptions, completion?: Completion, cleanup = false): Promise<MethodResults[M]> {
    let entry: Pending | undefined;
    let operationId: string | undefined;
    try {
      const data: unknown = params;
      operationId = options.operationId ?? (object(data) && typeof data.operation_id === 'string' ? data.operation_id : typeof method === 'string' && method.startsWith('operations.') && object(data) && typeof data.id === 'string' ? data.id : undefined);
      const timeoutMs = options.timeoutMs === undefined ? undefined : duration(options.timeoutMs, 0, 'timeoutMs');
      const deadline = timeoutMs === undefined ? undefined : performance.now() + timeoutMs;
      const details: ErrorDetails = { operationId, requestState: 'not_sent' };
      if (this.closing || this.failure || (this.quiescing && !cleanup)) throw new TransportError('Closed', 'Sidecar transport is closed', { ...details, cause: this.failure });
      if (options.signal?.aborted) throw new TransportError('Aborted', 'Request aborted before dispatch', { ...details, cause: options.signal.reason });
      if (timeoutMs === 0) throw new TransportError('DeadlineExceeded', 'Request deadline expired before dispatch', details);
      if (!Object.hasOwn(contracts, method) || (this.greeting && !this.supported.has(method))) throw new TransportError('UnsupportedMethod', `Sidecar does not support ${method}`, details);
      if (method !== 'server.shutdown' && this.pending.size >= this.pendingLimit) throw new TransportError('PendingLimit', 'Sidecar in-flight request limit reached', details);
      if (this.nextId > Number.MAX_SAFE_INTEGER) throw new TransportError('RequestIdExhausted', 'Safe request IDs have been exhausted', details);
      try {
        this.validate(`${method} params`, contracts[method].params, params);
      } catch (cause) {
        throw new ConfigurationError(`Invalid parameters for ${method}`, { ...details, cause });
      }
      const id = this.nextId++;
      let serialized: string;
      try {
        serialized = JSON.stringify({ id, method, params });
      } catch (cause) {
        throw new ConfigurationError('Request parameters must be JSON serializable', { ...details, requestId: id, cause });
      }
      const bytes = Buffer.byteLength(serialized);
      if (bytes > this.messageLimit) throw new TransportError('MessageTooLarge', 'Request exceeds negotiated message limit', { ...details, requestId: id });
      if (method !== 'server.shutdown' && this.queuedBytes + bytes + 1 > localMessageLimit + 1) throw new TransportError('PendingLimit', 'Buffered request byte limit reached', { ...details, requestId: id });
      const frame = Buffer.from(`${serialized}\n`);
      const { promise, resolve, reject } = Promise.withResolvers<MethodResults[M]>();
      entry = { id, method, operationId, sent: false, frame, resolve: value => resolve(value as MethodResults[M]), reject, completion, signal: options.signal, abort: undefined, timer: undefined };
      const current = entry;
      this.pending.set(id, current);
      this.queuedBytes += frame.length;
      if (options.signal) {
        current.abort = () => this.cancel(current, 'Aborted', options.signal?.reason);
        options.signal.addEventListener('abort', current.abort, { once: true });
      }
      if (deadline !== undefined) {
        const remaining = deadline - performance.now();
        if (remaining <= 0) this.cancel(current, 'DeadlineExceeded');
        else current.timer = setTimeout(() => this.cancel(current, 'DeadlineExceeded'), remaining);
      }
      if (this.pending.has(current.id) && (method === 'server.shutdown' || method === 'browser.close')) void this.beginClose(current).catch(() => {});
      else this.flush();
      return promise;
    } catch (error) {
      if (entry) this.remove(entry);
      if (error instanceof EokaError) {
        error.operationId ??= operationId;
        error.requestState ??= 'not_sent';
        error.stderr ??= this.stderr;
      }
      completion?.reject(error);
      return Promise.reject(error);
    }
  }

  private details(entry: Pending, stderr = this.stderr): ErrorDetails {
    return { requestId: entry.id, operationId: entry.operationId, requestState: entry.sent ? 'may_have_been_sent' : 'not_sent', stderr };
  }

  private detach(entry: Pending): void {
    clearTimeout(entry.timer);
    if (entry.signal && entry.abort) entry.signal.removeEventListener('abort', entry.abort);
    entry.timer = undefined;
    entry.signal = undefined;
    entry.abort = undefined;
  }

  private remove(entry: Pending): void {
    this.detach(entry);
    if (entry.frame) this.queuedBytes -= entry.frame.length;
    entry.frame = undefined;
    this.pending.delete(entry.id);
  }

  private cancel(entry: Pending, code: string, cause?: unknown): void {
    if (!entry.reject) return;
    const reject = entry.reject;
    this.detach(entry);
    entry.reject = undefined;
    entry.resolve = undefined;
    const error = new TransportError(code, entry.sent ? 'Local request cancelled after possible dispatch; the action was not rolled back' : 'Local request cancelled before dispatch', { ...this.details(entry), cause });
    reject(error);
    if (!entry.sent) {
      entry.completion?.reject(error);
      this.remove(entry);
    }
  }

  private flush(): void {
    if (this.blocked || this.closing) return;
    for (const entry of this.pending.values()) {
      if (!entry.frame) continue;
      const frame = entry.frame;
      this.queuedBytes -= frame.length;
      entry.frame = undefined;
      entry.sent = true;
      try {
        if (!this.child.stdin.write(frame)) {
          this.blocked = true;
          break;
        }
      } catch (cause) {
        this.fail(new TransportError('Write', 'Writing to the sidecar failed', { cause }));
        break;
      }
    }
  }

  private readonly onDrain = (): void => {
    this.blocked = false;
    this.flush();
  };

  private readonly onStderr = (chunk: Buffer): void => {
    const total = this.stderrBytes.length + chunk.length;
    this.stderrClipped ||= total > stderrLimit;
    if (chunk.length >= stderrLimit) this.stderrBytes = Buffer.from(chunk.subarray(chunk.length - stderrLimit));
    else {
      const keep = Math.min(this.stderrBytes.length, stderrLimit - chunk.length);
      this.stderrBytes = Buffer.concat([this.stderrBytes.subarray(this.stderrBytes.length - keep), chunk], keep + chunk.length);
    }
  };

  private readonly onData = (chunk: Buffer): void => {
    if (this.finished || this.failure) return;
    try {
      let offset = 0;
      while (offset < chunk.length) {
        const newline = chunk.indexOf(10, offset);
        const end = newline < 0 ? chunk.length : newline;
        const length = end - offset;
        if (this.frameLength + length > this.messageLimit + 1) throw new ProtocolError('Peer frame exceeds negotiated message limit');
        let frame: Buffer;
        if (this.frameLength === 0 && newline >= 0) {
          frame = chunk.subarray(offset, end);
          this.frameLength = length;
        } else {
          const needed = this.frameLength + length;
          if (needed > this.frame.length) {
            const capacity = Math.min(this.messageLimit + 1, Math.max(needed, this.frame.length * 2, 4096));
            const next = Buffer.allocUnsafe(capacity);
            this.frame.copy(next, 0, 0, this.frameLength);
            this.frame = next;
          }
          chunk.copy(this.frame, this.frameLength, offset, end);
          this.frameLength = needed;
          if (newline < 0) return;
          frame = this.frame.subarray(0, this.frameLength);
        }
        let frameEnd = frame.length;
        if (frameEnd > 0 && frame[frameEnd - 1] === 13) frameEnd--;
        if (frameEnd > this.messageLimit) throw new ProtocolError('Peer frame exceeds negotiated message limit');
        if (frameEnd === 0) throw new ProtocolError('Peer emitted an empty NDJSON frame');
        const text = this.decoder.decode(frameEnd === frame.length ? frame : frame.subarray(0, frameEnd));
        this.frameLength = 0;
        this.response(JSON.parse(text));
        offset = newline + 1;
      }
    } catch (cause) {
      this.fail(cause instanceof ProtocolError ? cause : new ProtocolError('Peer emitted malformed JSON or UTF-8', { cause }));
    }
  };

  private response(value: unknown): void {
    if (!object(value) || Object.keys(value).length !== 2 || !Object.hasOwn(value, 'id') || Object.hasOwn(value, 'result') === Object.hasOwn(value, 'error')) {
      throw new ProtocolError('Peer emitted an invalid response envelope');
    }
    if (Object.hasOwn(value, 'error')) this.validate('server error', bundle.types.ServerError, value.error);
    if (value.id === null && Object.hasOwn(value, 'error')) {
      throw new ProtocolError('Peer emitted an uncorrelated error', { cause: new ServerOperationError(value.error as ServerError) });
    }
    if (typeof value.id !== 'number' || !Number.isSafeInteger(value.id) || value.id <= 0) throw new ProtocolError('Peer response ID is not a safe positive integer');
    const entry = this.pending.get(value.id);
    if (!entry || !entry.sent) throw new ProtocolError('Peer response has an unknown or undispatched request ID');
    if (Object.hasOwn(value, 'error')) {
      const details = { ...this.details(entry), requestState: 'acknowledged' as const };
      const wireError = value.error as ServerError;
      if (wireError.data?.receipt) receiptEvidence(wireError.data.receipt);
      if (entry.operationId && wireError.data?.operation_id && entry.operationId !== wireError.data.operation_id) throw new ProtocolError('Server error has a different operation ID');
      const error = new ServerOperationError(wireError, details);
      if (entry.id === this.shutdownId) this.cleanupError = error;
      entry.reject?.(error);
      entry.completion?.reject(error);
    } else {
      const schema = contracts[entry.method].result;
      this.validate(`${entry.method} result`, schema, value.result);
      if (schema.title === 'OperationRecord') operationEvidence(value.result as OperationRecord, entry.operationId);
      else if (schema.title === 'Array_of_OperationRecord') for (const record of value.result as OperationRecord[]) operationEvidence(record);
      else if (schema.title === 'AgentStepResult') {
        const result = value.result as MethodResults['agent.step'];
        if (entry.operationId && result.operation_id !== entry.operationId) throw new ProtocolError('Agent response has a different operation ID');
        if (result.operation) operationEvidence(result.operation, result.operation_id);
      }
      if (entry.id === this.shutdownId) this.shutdownAcknowledged = true;
      entry.resolve?.(value.result);
      entry.completion?.resolve(value.result);
    }
    this.remove(entry);
    if (entry.id === this.shutdownId && entry.method === 'browser.close') this.child.stdin.end();
  }

  private rejectPending(error: EokaError): void {
    let stderr: string | undefined;
    for (const entry of this.pending.values()) {
      if (entry.id === this.shutdownId && !this.failure) continue;
      const details = { ...this.details(entry, stderr ??= this.stderr), cause: error };
      const local = error instanceof ProtocolError ? new ProtocolError(error.message, details) : new TransportError(error.code, error.message, details);
      entry.reject?.(local);
      if (this.failure || !entry.sent) {
        entry.completion?.reject(local);
        entry.completion = undefined;
      }
      entry.reject = undefined;
      entry.resolve = undefined;
      this.detach(entry);
      if (!entry.sent) this.remove(entry);
    }
  }

  private fail(error: EokaError): void {
    if (this.finished) return;
    this.failure ??= error;
    if (this.closing) this.cleanupError ??= error;
    this.rejectPending(error);
    if (this.closing && !this.child.stdin.writableEnded && !this.child.stdin.destroyed) {
      try {
        this.child.stdin.end();
      } catch (cause) {
        this.cleanupError = new TransportError('ShutdownWrite', 'Could not end sidecar input after a fatal failure', { cause: new AggregateError([this.cleanupError ?? error, cause]), stderr: this.stderr });
      }
    }
    void this.close().catch(() => {});
  }

  private readonly onEnd = (): void => {
    if (this.frameLength !== 0) this.fail(new ProtocolError('Peer ended with an unterminated NDJSON frame'));
    else if (!this.closing) this.fail(new TransportError('EOF', 'Sidecar stdout ended before transport disposal'));
  };

  private readonly onStreamError = (cause: Error): void => {
    this.fail(new TransportError('Stream', 'Sidecar stream failed', { cause, stderr: this.stderr }));
  };

  private readonly onProcessError = (cause: Error): void => {
    this.fail(new TransportError('Process', 'Sidecar process failed', { cause, stderr: this.stderr }));
  };

  private readonly onExit = (code: number | null, signal: NodeJS.Signals | null): void => {
    this.exited = true;
    if (code !== 0 || signal !== null) {
      const error = new TransportError('ProcessExit', `Sidecar exited with ${signal ?? `code ${code}`}`, { stderr: this.stderr });
      if (this.closing) this.cleanupError ??= error;
      else this.fail(error);
    } else if (!this.closing) this.fail(new TransportError('ProcessExit', 'Sidecar exited before transport disposal', { stderr: this.stderr }));
  };

  private readonly onClose = (): void => {
    this.childClosed = true;
    if (!this.closing) this.fail(new TransportError('ProcessExit', 'Sidecar process closed before disposal', { stderr: this.stderr }));
    if (this.escalating) {
      if (process.platform !== 'win32' && !this.ownedGroupAlive()) this.finishClose();
    } else if (process.platform !== 'win32' && this.ownedGroupAlive()) void this.terminate();
    else this.finishClose();
  };

  close(): Promise<void> {
    return this.beginClose();
  }

  whenClosed(): Promise<void> {
    return this.closed.promise;
  }

  get ownedProcessesTerminated(): boolean {
    return this.terminationConfirmed;
  }

  private beginClose(terminal?: Pending): Promise<void> {
    if (this.closeCompletion) return this.closeCompletion.promise;
    this.closeCompletion = Promise.withResolvers<void>();
    this.closing = true;
    if (terminal) this.shutdownId = terminal.id;
    this.rejectPending(new TransportError('Closed', 'Sidecar transport is closing'));
    if (this.finished) return this.closeCompletion.promise;
    try {
      if (terminal?.frame) {
        const frame = terminal.frame;
        this.queuedBytes -= frame.length;
        terminal.frame = undefined;
        terminal.sent = true;
        if (terminal.method === 'browser.close') this.child.stdin.write(frame);
        else this.child.stdin.end(frame);
      } else if (!this.failure && !this.exited && !this.child.stdin.destroyed && this.nextId <= Number.MAX_SAFE_INTEGER) {
        const id = this.nextId++;
        this.shutdownId = id;
        this.pending.set(id, { id, method: 'server.shutdown', operationId: undefined, sent: true, frame: undefined, resolve: undefined, reject: undefined, signal: undefined, abort: undefined, timer: undefined });
        this.child.stdin.end(`${JSON.stringify({ id, method: 'server.shutdown', params: {} })}\n`);
      } else this.child.stdin.end();
    } catch (cause) {
      this.cleanupError ??= new TransportError('ShutdownWrite', 'Could not send sidecar shutdown; cleanup may be incomplete', { cause, stderr: this.stderr });
    }
    this.teardownTimer = setTimeout(() => { void this.terminate(); }, this.shutdownTimeoutMs);
    return this.closeCompletion.promise;
  }

  private ownedGroupAlive(): boolean {
    if (this.child.pid === undefined) return false;
    try {
      process.kill(-this.child.pid, 0);
      return true;
    } catch (cause) {
      return !object(cause) || cause.code !== 'ESRCH';
    }
  }

  private async signalOwnedTree(force: boolean): Promise<boolean> {
    const pid = this.child.pid;
    if (pid === undefined) return false;
    if (process.platform !== 'win32') {
      try {
        process.kill(-pid, force ? 'SIGKILL' : 'SIGTERM');
        return true;
      } catch (cause) {
        if (object(cause) && cause.code === 'ESRCH') return false;
        throw cause;
      }
    }
    const root = process.env.SystemRoot ?? process.env.WINDIR;
    if (!root) throw new Error('Windows SystemRoot is required for owned-process-tree termination');
    const executable = join(root, 'System32', 'taskkill.exe');
    return new Promise<boolean>((resolve, reject) => {
      const killer = spawn(executable, ['/PID', String(pid), '/T', '/F'], { shell: false, stdio: 'ignore', windowsHide: true });
      const timer = setTimeout(() => {
        try {
          killer.kill('SIGKILL');
        } catch (cause) {
          reject(cause);
          return;
        }
        reject(new Error('Owned-process-tree termination command exceeded its deadline'));
      }, 1000);
      killer.once('error', cause => {
        clearTimeout(timer);
        reject(cause);
      });
      killer.once('close', code => {
        clearTimeout(timer);
        if (code === 0) resolve(true);
        else reject(new Error(`Owned-process-tree termination command exited with code ${code}`));
      });
    });
  }

  private async terminate(): Promise<void> {
    if (this.finished) return;
    clearTimeout(this.teardownTimer);
    this.teardownTimer = undefined;
    this.escalating = true;
    this.cleanupError ??= new TransportError('ShutdownTimeout', 'Graceful sidecar cleanup exceeded its deadline; file and profile cleanup is uncertain', { stderr: this.stderr });
    if (process.platform !== 'win32') {
      try {
        if (!await this.signalOwnedTree(false)) {
          this.finishClose();
          return;
        }
      } catch (cause) {
        this.cleanupError = new TransportError('Terminate', 'Could not terminate the owned sidecar process group', { cause: new AggregateError([this.cleanupError, cause]), stderr: this.stderr });
      }
    }
    this.teardownTimer = setTimeout(() => { void this.forceOwnedTree(); }, process.platform === 'win32' ? 0 : 1000);
  }

  private async forceOwnedTree(): Promise<void> {
    if (this.finished) return;
    try {
      const killed = await this.signalOwnedTree(true);
      if (process.platform === 'win32' && killed) this.windowsTreeTerminated = true;
      if (killed) this.cleanupError = new TransportError('ForcedKill', 'Owned sidecar process tree required forced termination; file and profile cleanup is uncertain', { cause: this.cleanupError, stderr: this.stderr });
    } catch (cause) {
      this.cleanupError = new TransportError('ForcedKill', 'Could not force-kill the owned sidecar process tree; cleanup is uncertain', { cause: new AggregateError([this.cleanupError, cause]), stderr: this.stderr });
    }
    this.teardownTimer = setTimeout(() => this.finishClose(), 1000);
  }

  private finishClose(): void {
    if (this.finished) return;
    this.terminationConfirmed = process.platform === 'win32'
      ? this.child.pid === undefined || this.windowsTreeTerminated || (this.childClosed && this.exited && this.shutdownAcknowledged && this.cleanupError === undefined)
      : !this.ownedGroupAlive();
    this.finished = true;
    clearTimeout(this.teardownTimer);
    this.teardownTimer = undefined;
    const stderr = this.stderr;
    for (const entry of this.pending.values()) {
      const error = new TransportError('Closed', 'Sidecar closed before native request completion; dispatch outcome is uncertain', { ...this.details(entry, stderr), cause: this.cleanupError ?? this.failure });
      entry.reject?.(error);
      entry.completion?.reject(error);
      this.remove(entry);
    }
    this.frame = Buffer.alloc(0);
    this.frameLength = 0;
    this.child.stdout.off('data', this.onData);
    this.child.stdout.off('end', this.onEnd);
    this.child.stdout.off('error', this.onStreamError);
    this.child.stderr.off('data', this.onStderr);
    this.child.stderr.off('error', this.onStreamError);
    this.child.stdin.off('error', this.onStreamError);
    this.child.stdin.off('drain', this.onDrain);
    this.child.off('error', this.onProcessError);
    this.child.off('exit', this.onExit);
    this.child.off('close', this.onClose);
    this.child.stdin.destroy();
    this.child.stdout.destroy();
    this.child.stderr.destroy();
    this.finalStderr = stderr;
    this.stderrBytes = Buffer.alloc(0);
    if (this.cleanupError) {
      this.cleanupError.stderr = this.stderr;
      this.closeCompletion?.reject(this.cleanupError);
      this.closed.reject(this.cleanupError);
    } else {
      this.closeCompletion?.resolve();
      this.closed.resolve();
    }
  }
}
