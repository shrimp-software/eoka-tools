import type { CallOptions, InvokeClient } from './client.js';
import { EokaError, type ErrorDetails } from './errors.js';
import type { Artifact, ErrorData, RecordingFailure, RecordingRecord } from './generated/protocol.js';

export function requireCaptureCapability(client: InvokeClient, method: string, capability: string): void {
  if (client.supports?.(method) !== true) {
    throw new EokaError('UnsupportedMethod', `Sidecar does not support ${method}.`, { requestState: 'not_sent' });
  }
  if (client.hasCapability?.(capability) !== true) {
    throw new EokaError('UnsupportedCapability', `Sidecar did not negotiate ${capability}.`, { requestState: 'not_sent' });
  }
}

export class RecordingError extends EokaError {
  readonly recordingId: string;
  readonly record: RecordingRecord | undefined;
  readonly failure: RecordingFailure | undefined;
  readonly path: string | undefined;
  readonly partialPath: string | undefined;
  readonly cleanupUncertain: boolean | undefined;

  constructor(code: string, message: string, recordingId: string, record?: RecordingRecord, details: ErrorDetails = {}, path?: string) {
    super(code, message, details);
    this.recordingId = recordingId;
    this.record = record;
    this.failure = record?.failure ?? undefined;
    const data = details.data as ErrorData | undefined;
    this.path = record?.path ?? data?.path ?? path;
    this.partialPath = record?.failure?.partialPath ?? data?.partialPath ?? undefined;
    this.cleanupUncertain = record?.failure?.cleanupUncertain ?? undefined;
  }
}

export function recordingFailure(error: unknown, id: string, record?: RecordingRecord, path?: string, pageId?: string): RecordingError {
  if (error instanceof RecordingError && error.recordingId === id) return error;
  const details: ErrorDetails = { cause: error };
  if (error instanceof EokaError) {
    if (error.requestId !== undefined) details.requestId = error.requestId;
    if (error.operationId !== undefined) details.operationId = error.operationId;
    if (error.requestState !== undefined) details.requestState = error.requestState;
    if (error.receipt !== undefined) details.receipt = error.receipt;
    if (error.data !== undefined) details.data = error.data;
    if (error.stderr !== undefined) details.stderr = error.stderr;
    const data = error.data as ErrorData | undefined;
    if (data?.recording != null) {
      try {
        validateRecordingRecord(data.recording, id, pageId ?? record?.pageId, path ?? record?.path);
      } catch (validation) {
        if (!(validation instanceof RecordingError)) throw validation;
        return new RecordingError(validation.code, validation.message, id, data.recording, details, path);
      }
      record = data.recording;
    }
  }
  return new RecordingError(
    error instanceof EokaError ? error.code : 'RecordingFailed',
    error instanceof Error ? error.message : String(error),
    id,
    record,
    details,
    path,
  );
}

export function validateRecordingRecord(record: RecordingRecord, id: string, pageId?: string, path?: string): void {
  if (record.recordingId !== id || (pageId !== undefined && record.pageId !== pageId) || (path !== undefined && record.path !== path)
    || (record.state === 'completed' && (record.artifact == null || record.artifact.path !== record.path || record.artifact.container !== 'mp4'
      || record.artifact.mimeType !== 'video/mp4' || !Number.isSafeInteger(record.artifact.bytes) || record.artifact.bytes <= 0 || record.failure != null))
    || (record.state !== 'completed' && record.artifact != null)) {
    throw new RecordingError('Protocol', `Received an inconsistent recording record for ${id}.`, id, record, { requestState: 'acknowledged' });
  }
}

export class Recording {
  readonly id: string;
  private readonly client: InvokeClient;
  private latest: RecordingRecord | undefined;

  constructor(client: InvokeClient, id: string, record?: RecordingRecord) {
    this.client = client;
    this.id = id;
    if (record !== undefined) validateRecordingRecord(record, id);
    this.latest = record;
  }

  get record(): RecordingRecord | undefined { return this.latest; }

  private remember(record: RecordingRecord): void {
    if (this.latest === undefined || this.latest.state === 'running' || this.latest.state === 'finalizing' || (record.state !== 'running' && record.state !== 'finalizing')) {
      this.latest = record;
    }
  }

  private async call(method: 'recording.get' | 'recording.stop' | 'recording.cancel', options?: CallOptions): Promise<RecordingRecord> {
    try {
      requireCaptureCapability(this.client, method, 'nativeRecording');
      const record = await this.client.invoke(method, { recordingId: this.id }, options);
      validateRecordingRecord(record, this.id, this.latest?.pageId, this.latest?.path);
      this.remember(record);
      return record;
    } catch (error) {
      const failure = recordingFailure(error, this.id, this.latest);
      if (failure.code !== 'Protocol' && failure.record?.recordingId === this.id && failure.record.pageId === (this.latest?.pageId ?? failure.record.pageId)) {
        this.remember(failure.record);
      }
      throw failure;
    }
  }

  status(options?: CallOptions): Promise<RecordingRecord> {
    return this.call('recording.get', options);
  }

  async stop(options?: CallOptions): Promise<Artifact> {
    const record = await this.call('recording.stop', options);
    if (record.state !== 'completed' || record.artifact == null) {
      throw new RecordingError(record.failure?.code ?? 'RecordingInvalidState', record.failure?.message ?? `Recording ${this.id} is ${record.state}; no completed artifact was published.`, this.id, record, { requestState: 'acknowledged' });
    }
    return record.artifact;
  }

  async cancel(options?: CallOptions): Promise<RecordingRecord> {
    const record = await this.call('recording.cancel', options);
    if (record.state === 'running' || record.state === 'finalizing') {
      throw new RecordingError('Protocol', `Cancellation of recording ${this.id} returned a nonterminal record.`, this.id, record, { requestState: 'acknowledged' });
    }
    return record;
  }
}
