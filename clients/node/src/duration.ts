import { ConfigurationError } from './errors.js';

export const maximumTimer = 2_147_483_647;
export const serverGraceMs = 10_000;

export function duration(value: number | undefined, fallback: number, name: string): number {
  const result = value ?? fallback;
  if (!Number.isFinite(result) || result < 0 || result > maximumTimer) {
    throw new ConfigurationError(`${name} must be finite and between 0 and ${maximumTimer}`, { requestState: 'not_sent' });
  }
  return result;
}
