import assert from 'node:assert/strict';
import test from 'node:test';
import { resolveCloudflareOptions } from '../dist/cloudflare.js';
import { ConfigurationError } from '../dist/errors.js';
import { Page } from '../dist/page.js';

test('resolves defaults and rejects invalid or client-side watcher options', () => {
  assert.equal(resolveCloudflareOptions(undefined), undefined);
  assert.equal(resolveCloudflareOptions(false), undefined);
  assert.deepEqual(resolveCloudflareOptions(true), { intervalMs: 500, timeoutMs: 30_000 });
  assert.deepEqual(resolveCloudflareOptions({ intervalMs: 1000, timeoutMs: 5000 }), { intervalMs: 1000, timeoutMs: 5000 });
  for (const options of [{ intervalMs: 99 }, { intervalMs: 60_001 }, { intervalMs: 1.5 }, { timeoutMs: 0 }, { timeoutMs: 120_001 }, { onSolved: () => {} }]) {
    assert.throws(() => resolveCloudflareOptions(options), ConfigurationError, JSON.stringify(Object.keys(options)));
  }
});

test('solveTurnstile is one server call whose local deadline outlasts the solve', async () => {
  const calls = [];
  const page = new Page({ invoke: async (method, params, options) => { calls.push({ method, params, options }); return { clicks: 1, elapsedMs: 900 }; } }, 'tab');
  assert.deepEqual(await page.solveTurnstile({ timeoutMs: 5000 }), { clicks: 1, elapsedMs: 900 });
  assert.deepEqual(await page.solveTurnstile(), { clicks: 1, elapsedMs: 900 });
  assert.deepEqual(calls.map(call => [call.method, call.params, call.options.timeoutMs]), [
    ['page.solve_turnstile', { pageId: 'tab', timeoutMs: 5000 }, 15_000],
    ['page.solve_turnstile', { pageId: 'tab', timeoutMs: 30_000 }, 40_000],
  ]);
});
