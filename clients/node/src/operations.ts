import type { CallOptions, InvokeClient } from './client.js';
import { EokaError, type ErrorDetails } from './errors.js';
import type { MethodParams, OperationRecord } from './generated/protocol.js';

export interface WaitOptions extends CallOptions {
  waitTimeoutMs?: number;
}

export class OperationError extends EokaError {
  readonly record: OperationRecord | undefined;

  constructor(code: string, message: string, operationId: string, record?: OperationRecord, details: ErrorDetails = {}) {
    const errorDetails: ErrorDetails = { ...details, operationId };
    if (details.receipt === undefined && record?.receipt != null) errorDetails.receipt = record.receipt;
    super(code, message, errorDetails);
    this.record = record;
  }
}

export function operationFailure(error: unknown, id: string, record?: OperationRecord): OperationError {
  if (error instanceof OperationError && error.operationId === id) return error;
  const details: ErrorDetails = { cause: error };
  if (error instanceof EokaError) {
    if (error.requestId !== undefined) details.requestId = error.requestId;
    if (error.requestState !== undefined) details.requestState = error.requestState;
    if (error.receipt !== undefined) details.receipt = error.receipt;
    if (error.data !== undefined) details.data = error.data;
    if (error.stderr !== undefined) details.stderr = error.stderr;
  }
  return new OperationError(
    error instanceof EokaError ? error.code : 'Operation',
    error instanceof Error ? error.message : String(error),
    id,
    record,
    details,
  );
}

export function validateOperationRecord(record: OperationRecord, id: string, targetId?: string): void {
  if (record.id !== id || (targetId !== undefined && record.target_id !== targetId) || (record.receipt != null && record.receipt.target_id !== record.target_id)) {
    throw new OperationError('Protocol', `Received an inconsistent operation record for ${id}${targetId === undefined ? '' : ` on page ${targetId}`}.`, id, record, { requestState: 'acknowledged' });
  }
}

export class Operation {
  readonly id: string;
  private readonly client: InvokeClient;
  private latest: OperationRecord | undefined;

  constructor(client: InvokeClient, id: string, record?: OperationRecord) {
    this.client = client;
    this.id = id;
    if (record !== undefined) validateOperationRecord(record, id);
    this.latest = record;
  }

  get record(): OperationRecord | undefined {
    return this.latest;
  }

  async get(options?: CallOptions): Promise<OperationRecord> {
    try {
      const record = await this.client.invoke('operations.get', { id: this.id }, { ...options, operationId: this.id });
      validateOperationRecord(record, this.id, this.latest?.target_id);
      this.latest = record;
      return record;
    } catch (error) {
      throw operationFailure(error, this.id, this.latest);
    }
  }

  async wait(options: WaitOptions = {}): Promise<OperationRecord> {
    const { waitTimeoutMs, ...localOptions } = options;
    try {
      const params: MethodParams['operations.wait'] = { id: this.id };
      if (waitTimeoutMs !== undefined) params.timeout_ms = waitTimeoutMs;
      localOptions.operationId = this.id;
      const record = await this.client.invoke('operations.wait', params, localOptions);
      validateOperationRecord(record, this.id, this.latest?.target_id);
      this.latest = record;
      return record;
    } catch (error) {
      throw operationFailure(error, this.id, this.latest);
    }
  }

  async cancel(options?: CallOptions): Promise<OperationRecord> {
    try {
      const record = await this.client.invoke('operations.cancel', { id: this.id }, { ...options, operationId: this.id });
      validateOperationRecord(record, this.id, this.latest?.target_id);
      this.latest = record;
      return record;
    } catch (error) {
      throw operationFailure(error, this.id, this.latest);
    }
  }
}
