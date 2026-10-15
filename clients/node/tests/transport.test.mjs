import assert from 'node:assert/strict';
import { once, getEventListeners } from 'node:events';
import { spawn } from 'node:child_process';
import { readFile } from 'node:fs/promises';
import { fileURLToPath } from 'node:url';
import test from 'node:test';
import { SidecarTransport } from '../dist/transport.js';
import { EokaError, ProtocolError, ServerOperationError, TransportError } from '../dist/errors.js';
import { protocolVersion } from '../dist/generated/protocol.js';

const serverPath = fileURLToPath(new URL('../fixtures/wire/transport-peer.mjs', import.meta.url));
const operationId = 'cddc7960-14b3-4c8e-b38c-d0b281be221e';

async function start(t, mode = 'normal', env = {}, options = {}) {
  const transport = await SidecarTransport.start({
    serverPath, timeoutMs: 3000, shutdownTimeoutMs: 500,
    env: { ...process.env, EOKA_TRANSPORT_TEST_MODE: mode, ...env },
    ...options,
  });
  t.after(() => transport.close().catch(() => {}));
  return transport;
}

function evaluate(transport, js = 'null', options = {}) {
  return transport.invoke('page.evaluate', { pageId: 'page', js }, { timeoutMs: 1500, ...options });
}

function clean(transport) {
  assert.equal(transport.pending.size, 0);
  assert.equal(transport.child.stdout.listenerCount('data'), 0);
  assert.equal(transport.child.stdin.listenerCount('drain'), 0);
  assert.equal(transport.child.listenerCount('exit'), 0);
  assert.equal(transport.teardownTimer?.hasRef() ?? false, false);
}

test('handshake uses generated protocol and actual advertised support', async t => {
  const transport = await start(t, 'unsupported');
  assert.deepEqual(transport.hello.protocol, protocolVersion);
  assert.deepEqual(transport.hello.capabilities, ['fixture']);
  await assert.rejects(evaluate(transport), error => error.code === 'UnsupportedMethod' && error.requestState === 'not_sent');
  await transport.close();
  clean(transport);
});

test('whenClosed observes teardown without initiating it and settles only after child closure', async t => {
  const transport = await start(t);
  const closure = transport.whenClosed();
  assert.equal(transport.whenClosed(), closure);
  assert.equal(transport.ownedProcessesTerminated, false);
  let settled = false;
  const observed = closure.then(() => { settled = true; });
  assert.deepEqual(await evaluate(transport), { result: null });
  assert.equal(settled, false);
  assert.equal(transport.closing, false);
  await transport.invoke('server.shutdown', {});
  await observed;
  assert.equal(transport.finished, true);
  assert.equal(transport.child.exitCode, 0);
  assert.equal(transport.ownedProcessesTerminated, true);
  clean(transport);
});

test('incremental framing preserves UTF-8 split inside multibyte characters', async t => {
  const transport = await start(t, 'split-utf8');
  assert.deepEqual(await evaluate(transport), { result: 'é🙂漢字' });
  await transport.close();
  clean(transport);
});

for (const mode of ['boundary-lf', 'boundary-crlf', 'boundary-split-crlf']) {
  test(`${mode} accepts a response at the exact negotiated byte limit`, async t => {
    const limit = 2048;
    const transport = await start(t, mode, { EOKA_TRANSPORT_TEST_MESSAGE_BYTES: String(limit) });
    const overhead = Buffer.byteLength(JSON.stringify({ id: 2, result: { result: '' } }));
    assert.deepEqual(await evaluate(transport), { result: 'x'.repeat(limit - overhead) });
    assert.deepEqual(await transport.invoke('operations.list', {}), []);
    await transport.close();
    clean(transport);
  });
}

test('out-of-order responses correlate null and array JSON results', async t => {
  const transport = await start(t, 'out-of-order');
  const nullResult = evaluate(transport, 'null');
  const arrayResult = evaluate(transport, '[1,null,"é"]');
  assert.deepEqual(await arrayResult, { result: [1, null, 'é'] });
  assert.deepEqual(await nullResult, { result: null });
  assert.deepEqual(await transport.invoke('operations.list', {}), []);
  await transport.close();
  clean(transport);
});

for (const mode of ['malformed', 'array-envelope', 'dual-envelope', 'invalid-utf8', 'oversized', 'boundary-oversized-lf', 'boundary-oversized-crlf', 'boundary-oversized-split-crlf', 'empty-frame', 'empty-crlf', 'unknown-id', 'null-error', 'invalid-result', 'partial-eof']) {
  test(`${mode} rejects all pending calls and tears down the peer`, async t => {
    const transport = await start(t, mode, { EOKA_TRANSPORT_TEST_MESSAGE_BYTES: '2048' });
    const signal = new AbortController();
    const calls = [evaluate(transport, 'null', { signal: signal.signal, operationId }), evaluate(transport, '[]')];
    const results = await Promise.allSettled(calls);
    for (const result of results) {
      assert.equal(result.status, 'rejected');
      assert.ok(result.reason instanceof ProtocolError);
      assert.equal(result.reason.requestState, 'may_have_been_sent');
      assert.ok(result.reason.requestId > 0);
    }
    assert.equal(results[0].reason.operationId, operationId);
    assert.equal(getEventListeners(signal.signal, 'abort').length, 0);
    await transport.close().catch(() => {});
    clean(transport);
  });
}

for (const mode of ['eof', 'crash']) {
  test(`${mode} settles dispatched calls and retains bounded stderr evidence`, async t => {
    const transport = await start(t, mode);
    const result = await Promise.allSettled([evaluate(transport, 'null', { operationId })]);
    const error = result[0].reason;
    assert.ok(error instanceof TransportError);
    assert.equal(error.requestState, 'may_have_been_sent');
    assert.equal(error.operationId, operationId);
    if (mode === 'crash') {
      assert.match(error.stderr, /crash-tail$/);
      assert.ok(Buffer.byteLength(error.stderr) <= 16384);
    }
    await transport.close().catch(() => {});
    clean(transport);
  });
}

test('abort before dispatch releases its slot and preserves caller identity', async t => {
  const transport = await start(t);
  const controller = new AbortController();
  const reason = new Error('caller abort');
  controller.abort(reason);
  await assert.rejects(evaluate(transport, 'null', { signal: controller.signal, operationId }), error => {
    assert.equal(error.code, 'Aborted');
    assert.equal(error.operationId, operationId);
    assert.equal(error.requestState, 'not_sent');
    assert.equal(error.requestId, undefined);
    assert.equal(error.cause, reason);
    return true;
  });
  assert.equal(transport.pending.size, 0);
  assert.equal(getEventListeners(controller.signal, 'abort').length, 0);
  assert.deepEqual(await evaluate(transport), { result: null });
});

test('deadline after dispatch retains its slot until a correlated late response', async t => {
  const transport = await start(t, 'late', { EOKA_TRANSPORT_TEST_IN_FLIGHT: '1' });
  assert.deepEqual(await evaluate(transport), { result: null });
  const controller = new AbortController();
  await assert.rejects(evaluate(transport, 'null', { timeoutMs: 10.5, signal: controller.signal, operationId }), error => {
    assert.equal(error.code, 'DeadlineExceeded');
    assert.equal(error.requestState, 'may_have_been_sent');
    assert.equal(error.operationId, operationId);
    assert.equal(error.receipt, undefined);
    return true;
  });
  assert.equal(getEventListeners(controller.signal, 'abort').length, 0);
  assert.equal(transport.pending.size, 1);
  await assert.rejects(evaluate(transport), error => error.code === 'PendingLimit' && error.requestState === 'not_sent');
  await once(transport.child.stdout, 'data');
  assert.equal(transport.pending.size, 0);
  assert.equal(transport.failure, undefined);
  assert.deepEqual(await evaluate(transport), { result: null });
});

test('backpressure cancellation differentiates queued and dispatched work without replay', async t => {
  const transport = await start(t, 'backpressure', { EOKA_TRANSPORT_TEST_IN_FLIGHT: '2' });
  const dispatched = new AbortController();
  const queued = new AbortController();
  const first = evaluate(transport, 'null' + ' '.repeat(1024 * 1024), { signal: dispatched.signal, operationId });
  assert.equal(transport.blocked, true);
  const second = evaluate(transport, 'null', { signal: queued.signal });
  const settlements = Promise.allSettled([first, second]);
  dispatched.abort();
  queued.abort();
  const results = await settlements;
  assert.equal(results[0].reason.requestState, 'may_have_been_sent');
  assert.equal(results[0].reason.operationId, operationId);
  assert.equal(results[1].reason.requestState, 'not_sent');
  assert.equal(transport.pending.size, 1);
  assert.equal(transport.queuedBytes, 0);
  assert.equal(getEventListeners(dispatched.signal, 'abort').length, 0);
  assert.equal(getEventListeners(queued.signal, 'abort').length, 0);
  assert.deepEqual(await evaluate(transport), { result: { index: 2 } });
  assert.equal(transport.pending.size, 0);
});

test('outbound negotiated bounds and invalid params reject before dispatch', async t => {
  const transport = await start(t, 'normal', { EOKA_TRANSPORT_TEST_MESSAGE_BYTES: '2048' });
  await assert.rejects(evaluate(transport, 'x'.repeat(4096)), error => error.code === 'MessageTooLarge' && error.requestState === 'not_sent');
  await assert.rejects(transport.invoke('page.evaluate', { pageId: 'page' }), error => error.code === 'Configuration' && error.requestState === 'not_sent');
  await assert.rejects(evaluate(transport, 'null', { timeoutMs: 0 }), error => error.code === 'DeadlineExceeded' && error.requestState === 'not_sent');
  assert.equal(transport.pending.size, 0);
  assert.deepEqual(await evaluate(transport), { result: null });
});

test('structured server operation errors preserve wire receipts and IDs', async t => {
  const transport = await start(t, 'operation-error');
  await assert.rejects(transport.invoke('actions.perform', {
    pageId: 'page', operation_id: operationId,
    action: { kind: 'click', target: { query: { kind: 'css', selector: 'button' } } },
  }), error => {
    assert.ok(error instanceof ServerOperationError);
    assert.ok(error instanceof EokaError);
    assert.equal(error.code, 'Action');
    assert.equal(error.operationId, operationId);
    assert.equal(error.requestState, 'acknowledged');
    assert.equal(error.receipt.outcome, 'outcome_unknown');
    assert.equal(error.receipt.dispatch, 'may_have_been_sent');
    assert.equal(error.data, error.wireError.data);
    return true;
  });
  assert.equal(transport.pending.size, 0);
});

test('schema-valid verified operation lacking evidence is rejected', async t => {
  const transport = await start(t, 'invalid-operation');
  await assert.rejects(transport.invoke('operations.get', { id: operationId }), ProtocolError);
  await transport.close().catch(() => {});
  clean(transport);
});

test('shared close bypasses the in-flight limit and rejects local waits immediately', async t => {
  const transport = await start(t, 'late', { EOKA_TRANSPORT_TEST_IN_FLIGHT: '1' });
  const controller = new AbortController();
  const waiting = evaluate(transport, 'null', { signal: controller.signal });
  const settled = assert.rejects(waiting, error => error.code === 'Closed' && error.requestState === 'may_have_been_sent');
  const close = transport.close();
  assert.equal(transport.close(), close);
  await settled;
  assert.equal(getEventListeners(controller.signal, 'abort').length, 0);
  await close;
  clean(transport);
});

test('shutdown preserves server cleanup failures', async t => {
  const transport = await start(t, 'shutdown-error');
  const closureError = assert.rejects(transport.whenClosed(), error => error instanceof ServerOperationError && error.data.detail.code === 'Cleanup');
  await assert.rejects(transport.close(), error => error instanceof ServerOperationError && error.data.detail.code === 'Cleanup');
  await closureError;
  clean(transport);
});

test('bounded teardown escalates only its owned sidecar and reports uncertainty', { timeout: 5000 }, async t => {
  const transport = await SidecarTransport.start({ serverPath, shutdownTimeoutMs: 15, env: { ...process.env, EOKA_TRANSPORT_TEST_MODE: 'stubborn' } });
  t.after(() => transport.close().catch(() => {}));
  const close = transport.close();
  assert.equal(transport.close(), close);
  await assert.rejects(close, error => error.code === 'ForcedKill' && /cleanup is uncertain/.test(error.message));
  assert.equal(transport.child.signalCode, 'SIGKILL');
  clean(transport);
});

for (const mode of ['incompatible-protocol', 'invalid-limits']) {
  test(`${mode} fails handshake and bounds startup cleanup`, { timeout: 5000 }, async () => {
    await assert.rejects(SidecarTransport.start({
      serverPath, timeoutMs: 3000, shutdownTimeoutMs: 500,
      env: { ...process.env, EOKA_TRANSPORT_TEST_MODE: mode },
    }), ProtocolError);
  });
}

test('startup abort before spawn and handshake deadline are typed', { timeout: 5000 }, async () => {
  const controller = new AbortController();
  controller.abort('stop');
  await assert.rejects(SidecarTransport.start({ serverPath: '/does-not-exist', signal: controller.signal }), error => error.code === 'Aborted' && error.requestState === 'not_sent');
  await assert.rejects(SidecarTransport.start({
    serverPath, timeoutMs: 100, shutdownTimeoutMs: 500,
    env: { ...process.env, EOKA_TRANSPORT_TEST_MODE: 'handshake-hang' },
  }), error => error.code === 'DeadlineExceeded');
});

test('tracked native completion survives dispatched local abort without replay', { timeout: 5000 }, async t => {
  const transport = await start(t, 'late', { EOKA_TRANSPORT_TEST_IN_FLIGHT: '1' });
  const controller = new AbortController();
  const tracked = transport.invokeTracked('page.evaluate', { pageId: 'page', js: 'null' }, { signal: controller.signal, operationId });
  controller.abort();
  await assert.rejects(tracked.result, error => error.code === 'Aborted' && error.requestState === 'may_have_been_sent');
  assert.equal(transport.pending.size, 1);
  assert.deepEqual(await tracked.completion, { result: null });
  assert.equal(transport.pending.size, 0);
  assert.equal(transport.nextId, 3);
});

test('tracked unsent work and peer failures settle both promises', { timeout: 5000 }, async t => {
  const transport = await start(t, 'malformed');
  const controller = new AbortController();
  controller.abort();
  const unsent = transport.invokeTracked('page.evaluate', { pageId: 'page', js: 'null' }, { signal: controller.signal });
  const unsentResults = await Promise.allSettled([unsent.result, unsent.completion]);
  for (const result of unsentResults) {
    assert.equal(result.status, 'rejected');
    assert.equal(result.reason.requestState, 'not_sent');
  }
  const sent = transport.invokeTracked('page.evaluate', { pageId: 'page', js: 'null' });
  const sentResults = await Promise.allSettled([sent.result, sent.completion]);
  for (const result of sentResults) {
    assert.equal(result.status, 'rejected');
    assert.ok(result.reason instanceof ProtocolError);
    assert.equal(result.reason.requestState, 'may_have_been_sent');
  }
});

test('teardown settles abandoned tracked native completion', { timeout: 5000 }, async t => {
  const transport = await start(t, 'late');
  const controller = new AbortController();
  const tracked = transport.invokeTracked('page.evaluate', { pageId: 'page', js: 'null' }, { signal: controller.signal });
  controller.abort();
  await assert.rejects(tracked.result, error => error.code === 'Aborted');
  const completion = assert.rejects(tracked.completion, error => error.code === 'Closed' && error.requestState === 'may_have_been_sent');
  await transport.close();
  await completion;
  clean(transport);
});

test('quiescence rejects local waits while allowing bounded exact-tab cleanup after native completion', { timeout: 5000 }, async t => {
  const transport = await start(t, 'late', { EOKA_TRANSPORT_TEST_IN_FLIGHT: '1' });
  const tracked = transport.invokeTracked('page.evaluate', { pageId: 'page', js: 'null' });
  transport.quiesce();
  await assert.rejects(tracked.result, error => error.code === 'Closed' && error.requestState === 'may_have_been_sent');
  await assert.rejects(evaluate(transport), error => error.code === 'Closed' && error.requestState === 'not_sent');
  assert.deepEqual(await tracked.completion, { result: null });
  assert.deepEqual(await transport.invokeCleanup('browser.close_tab', { pageId: 'exact-page' }, { timeoutMs: 1000 }), {});
  await assert.rejects(transport.invokeCleanup('page.evaluate', { pageId: 'page', js: 'null' }), error => error.code === 'Configuration' && error.requestState === 'not_sent');
  await transport.close();
  clean(transport);
});

test('hello followed by malformed framing in the same peer write rejects startup', { timeout: 5000 }, async () => {
  await assert.rejects(SidecarTransport.start({
    serverPath, timeoutMs: 3000, shutdownTimeoutMs: 500,
    env: { ...process.env, EOKA_TRANSPORT_TEST_MODE: 'hello-malformed' },
  }), error => error instanceof ProtocolError && /malformed JSON/.test(error.message));
});

for (const mode of ['secret-stderr', 'secret-stderr-clipped']) {
test(`${mode} redacts credentials across chunk boundaries and truncated tails`, { timeout: 5000 }, async t => {
  const transport = await start(t, mode, {}, { redactionValues: ['configured-proxy-secret-without-any-label'] });
  await assert.rejects(evaluate(transport), error => {
    assert.ok(error instanceof TransportError);
    assert.match(error.stderr, /benign final line/);
    assert.ok(Buffer.byteLength(error.stderr) <= 16384);
    assert.doesNotMatch(error.stderr, /url-user|url-password|url-query-secret|url-fragment-secret|bearer-secret|token-secret|password-secret|json-auth-secret|json-password-secret|configured-proxy-secret|devtools-secret|cookie-secret|api-key-secret|partial-leading-secret|partial-trailing-secret/);
    assert.doesNotMatch(error.stderr, /environment-api-secret|header-token-secret|quoted secret value/);
    if (mode === 'secret-stderr') {
      assert.match(error.stderr, /libX\.so: not found/);
      assert.match(error.stderr, /Chrome sandbox failed: permission denied/);
      assert.match(error.stderr, /DevTools listening on ws:\/\/127\.0\.0\.1:9222\/devtools\/\[REDACTED\]/);
      assert.match(error.stderr, /Authorization: \[REDACTED\]/);
      assert.match(error.stderr, /https:\/\/example\.test\/path\?\[REDACTED\]#\[REDACTED\]/);
    }
    return true;
  });
  await transport.close().catch(error => {
    assert.match(error.stderr, /benign final line/);
    assert.doesNotMatch(error.stderr, /secret|url-user|url-password/);
  });
  clean(transport);
});
}

async function running(pid) {
  try {
    process.kill(pid, 0);
    if (process.platform === 'linux') {
      const stat = await readFile(`/proc/${pid}/stat`, 'utf8');
      return stat.slice(stat.lastIndexOf(')') + 2).split(' ')[0] !== 'Z';
    }
    return true;
  } catch (error) {
    if (error.code === 'ESRCH' || error.code === 'ENOENT') return false;
    throw error;
  }
}

for (const mode of ['owned-descendant', 'owned-descendant-exiting']) {
  test(`${mode} forced teardown kills its private group and preserves external processes`, { timeout: 6000, skip: process.platform === 'win32' }, async t => {
    const descendantPath = fileURLToPath(new URL('../fixtures/wire/transport-descendant.mjs', import.meta.url));
    const external = spawn(process.execPath, [descendantPath], { detached: true, stdio: ['ignore', 'ignore', 'ignore', 'ipc'] });
    t.after(async () => {
      if (external.exitCode === null && external.signalCode === null) {
        const exit = once(external, 'exit');
        external.kill('SIGKILL');
        await exit;
      }
    });
    await once(external, 'message');
    const transport = await start(t, mode, {}, { shutdownTimeoutMs: 15 });
    const ownedPid = transport.hello.ownedDescendantPid;
    assert.equal(typeof ownedPid, 'number');
    assert.equal(await running(ownedPid), true);
    assert.equal(await running(external.pid), true);
    await assert.rejects(transport.close(), error => error.code === 'ForcedKill' && /cleanup is uncertain/.test(error.message));
    assert.equal(await running(ownedPid), false);
    assert.equal(await running(external.pid), true);
    clean(transport);
  });
}

for (const method of ['server.shutdown', 'browser.close']) {
  test(`${method} shares disposal without writing another shutdown to a closed reader`, { timeout: 5000 }, async t => {
    const transport = await start(t, 'terminal-reader-closed');
    const invocation = transport.invoke(method, {});
    const close = transport.close();
    assert.equal(transport.close(), close);
    assert.deepEqual(await invocation, {});
    await close;
    await transport.whenClosed();
    assert.equal(transport.nextId, 3);
    assert.equal(transport.cleanupError, undefined);
    assert.equal(transport.failure, undefined);
    clean(transport);
  });

  test(`${method} missing terminal response settles its wire caller`, { timeout: 5000 }, async t => {
    const transport = await start(t, 'terminal-eof-no-reply');
    const invocation = transport.invoke(method, {});
    const rejection = assert.rejects(invocation, error => error.code === 'Closed' && error.requestState === 'may_have_been_sent');
    await transport.close();
    await rejection;
    clean(transport);
  });
}

test('raw browser.close preserves native cleanup errors through shared teardown', { timeout: 5000 }, async t => {
  const transport = await start(t, 'terminal-browser-error');
  const invocation = transport.invoke('browser.close', {});
  const close = transport.close();
  const invocationError = assert.rejects(invocation, error => error instanceof ServerOperationError && error.data.detail.code === 'BrowserCleanup');
  const closeError = assert.rejects(close, error => error instanceof ServerOperationError && error.data.detail.code === 'BrowserCleanup');
  const closureError = assert.rejects(transport.whenClosed(), error => error instanceof ServerOperationError && error.data.detail.code === 'BrowserCleanup');
  await Promise.all([invocationError, closeError, closureError]);
  assert.equal(transport.nextId, 3);
  clean(transport);
});

test('raw server.shutdown retains its reserved control admission at the pending limit', { timeout: 5000 }, async t => {
  const transport = await start(t, 'late', { EOKA_TRANSPORT_TEST_IN_FLIGHT: '1' });
  const pending = evaluate(transport);
  const rejection = assert.rejects(pending, error => error.code === 'Closed' && error.requestState === 'may_have_been_sent');
  const shutdown = transport.invoke('server.shutdown', {});
  assert.deepEqual(await shutdown, {});
  await rejection;
  await transport.close();
  clean(transport);
});

for (const method of ['server.shutdown', 'browser.close']) {
  test(`${method} fatal peer response rejects its terminal caller and initiates EOF cleanup`, { timeout: 5000 }, async t => {
    const transport = await start(t, 'terminal-malformed');
    await assert.rejects(transport.invoke(method, {}), ProtocolError);
    assert.equal(transport.child.stdin.writableEnded, true);
    await transport.close().catch(error => assert.ok(error instanceof ProtocolError));
    clean(transport);
  });
}
