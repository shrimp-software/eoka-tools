import type { ActionReceipt, ServerError } from './generated/protocol.js';

export type RequestState = 'not_sent' | 'may_have_been_sent' | 'acknowledged';

export interface ErrorDetails {
  requestId?: number | undefined;
  operationId?: string | undefined;
  requestState?: RequestState | undefined;
  receipt?: ActionReceipt | undefined;
  data?: unknown;
  stderr?: string | undefined;
  cause?: unknown;
}

export class EokaError extends Error {
  readonly code: string;
  requestId?: number | undefined;
  operationId?: string | undefined;
  requestState?: RequestState | undefined;
  receipt?: ActionReceipt | undefined;
  data?: unknown;
  stderr?: string | undefined;

  constructor(code: string, message: string, details: ErrorDetails = {}) {
    super(message, { cause: details.cause });
    this.name = new.target.name;
    this.code = code;
    this.requestId = details.requestId;
    this.operationId = details.operationId;
    this.requestState = details.requestState;
    this.receipt = details.receipt;
    this.data = details.data;
    this.stderr = details.stderr;
  }
}

export class ConfigurationError extends EokaError {
  constructor(message: string, details: ErrorDetails = {}) {
    super('Configuration', message, details);
  }
}

export class TransportError extends EokaError {}

export class ProtocolError extends EokaError {
  constructor(message: string, details: ErrorDetails = {}) {
    super('Protocol', message, details);
  }
}

export class ServerOperationError extends EokaError {
  readonly wireError: ServerError;

  constructor(wireError: ServerError, details: ErrorDetails = {}) {
    super(wireError.code, wireError.message, {
      ...details,
      data: wireError.data,
      operationId: wireError.data?.operation_id ?? details.operationId,
      receipt: wireError.data?.receipt ?? details.receipt,
    });
    this.wireError = wireError;
  }
}
