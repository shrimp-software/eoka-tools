import assert from 'node:assert/strict';
import test from 'node:test';
import { Page } from '../dist/index.js';
import { catalogMethods } from '../dist/generated/catalog.js';
import { methodNames } from '../dist/generated/protocol.js';

test('page.ops exposes every catalog operation and addresses this page', async () => {
  const calls = [];
  const page = new Page({ invoke: async (method, params, options) => { calls.push({ method, params, options }); return method; } }, 'page-1');

  assert.equal(Object.keys(page.ops).length, catalogMethods.length);
  for (const method of catalogMethods) assert.ok(methodNames.includes(method), `${method} is not a server method`);

  assert.equal(await page.ops.console({ clear: true }, { timeoutMs: 5 }), 'console');
  assert.equal(await page.ops.cookies(), 'cookies');
  assert.equal(await page.ops.networkRecordStart(), 'network.record.start');
  assert.deepEqual(calls, [
    { method: 'console', params: { clear: true, pageId: 'page-1' }, options: { timeoutMs: 5 } },
    { method: 'cookies', params: { pageId: 'page-1' }, options: undefined },
    { method: 'network.record.start', params: { pageId: 'page-1' }, options: undefined },
  ]);
});
