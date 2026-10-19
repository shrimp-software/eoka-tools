import assert from 'node:assert/strict';
import { setTimeout as delay } from 'node:timers/promises';
import test from 'node:test';
import {
  CloudflareWatcher,
  isCloudflareChallengeTitle,
  resolveCloudflareOptions,
} from '../dist/cloudflare.js';
import { ConfigurationError } from '../dist/errors.js';

test('recognizes full-page Cloudflare challenge titles', () => {
  for (const title of [
    'Just a moment...',
    'Attention Required! | Cloudflare',
    'Checking your browser before accessing example.com.',
    'Verify you are human',
    'Please wait... | Cloudflare',
  ]) assert.equal(isCloudflareChallengeTitle(title), true, title);

  for (const title of ['Example Domain', 'Sign in | Cloudflare', 'Please wait for the download']) {
    assert.equal(isCloudflareChallengeTitle(title), false, title);
  }
});

test('resolves defaults and rejects invalid watcher options', () => {
  assert.equal(resolveCloudflareOptions(undefined), undefined);
  assert.equal(resolveCloudflareOptions(false), undefined);
  assert.deepEqual(resolveCloudflareOptions(true), { intervalMs: 500, timeoutMs: 30_000 });
  assert.deepEqual(resolveCloudflareOptions({ intervalMs: 1000, timeoutMs: 5000 }), { intervalMs: 1000, timeoutMs: 5000 });
  assert.throws(() => resolveCloudflareOptions({ intervalMs: 99 }), ConfigurationError);
  assert.throws(() => resolveCloudflareOptions({ intervalMs: 60_001 }), ConfigurationError);
  assert.throws(() => resolveCloudflareOptions({ timeoutMs: 0 }), ConfigurationError);
  assert.throws(() => resolveCloudflareOptions({ onSolved: 'no' }), ConfigurationError);
});

test('clicks a detected challenge once and stops polling it after resolution', async (t) => {
  let title = 'Just a moment...';
  let solves = 0;
  const page = {
    async title() { return title; },
    async solveTurnstile() {
      solves += 1;
      title = 'Example Domain';
    },
  };
  const solved = Promise.withResolvers();
  const watcher = new CloudflareWatcher(
    { tabs: async () => [{ id: 'tab' }], page: () => page },
    { intervalMs: 100, timeoutMs: 1000, onSolved: result => solved.resolve(result) },
  );
  t.after(() => watcher.stop());
  watcher.start();
  assert.equal(await solved.promise, page);
  await delay(250);
  assert.equal(solves, 1);
});

test('does not click a normal page', async (t) => {
  let solves = 0;
  const page = {
    async title() { return 'Example Domain'; },
    async solveTurnstile() { solves += 1; },
  };
  const watcher = new CloudflareWatcher(
    { tabs: async () => [{ id: 'tab' }], page: () => page },
    { intervalMs: 100, timeoutMs: 1000 },
  );
  t.after(() => watcher.stop());
  watcher.start();
  await delay(300);
  assert.equal(solves, 0);
});
