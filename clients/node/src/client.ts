import type { MethodName, MethodParams, MethodResults } from './generated/protocol.js';

export interface CallOptions {
  signal?: AbortSignal;
  timeoutMs?: number;
  operationId?: string;
}

export interface InvokeClient {
  invoke<M extends MethodName>(method: M, params: MethodParams[M], options?: CallOptions): Promise<MethodResults[M]>;
  supports?(method: string): boolean;
  hasCapability?(name: string): boolean;
}
