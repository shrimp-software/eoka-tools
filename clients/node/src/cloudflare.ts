import { ConfigurationError } from './errors.js';

/** Options for the server-side Cloudflare challenge watcher. */
export interface CloudflareOptions {
  /** Poll interval in milliseconds. Defaults to 500. */
  intervalMs?: number;
  /** Deadline for each challenge solve. Defaults to 30000. */
  timeoutMs?: number;
}

export interface ResolvedCloudflareOptions {
  intervalMs: number;
  timeoutMs: number;
}

export function resolveCloudflareOptions(value: boolean | CloudflareOptions | undefined): ResolvedCloudflareOptions | undefined {
  if (value === undefined || value === false) return undefined;
  const options = value === true ? {} : value;
  for (const name of Object.keys(options)) {
    if (name !== 'intervalMs' && name !== 'timeoutMs') throw new ConfigurationError(`autoSolveCloudflare.${name} is not supported; the server solves challenges and reports them in its log`);
  }
  const intervalMs = options.intervalMs ?? 500;
  const timeoutMs = options.timeoutMs ?? 30_000;
  if (!Number.isInteger(intervalMs) || intervalMs < 100 || intervalMs > 60_000) {
    throw new ConfigurationError('autoSolveCloudflare.intervalMs must be an integer between 100 and 60000 milliseconds');
  }
  if (!Number.isInteger(timeoutMs) || timeoutMs < 1 || timeoutMs > 120_000) {
    throw new ConfigurationError('autoSolveCloudflare.timeoutMs must be an integer between 1 and 120000 milliseconds');
  }
  return { intervalMs, timeoutMs };
}
