import assert from 'node:assert/strict';
import { resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { test } from 'node:test';
import { Browser } from '../dist/browser.js';
import { Page } from '../dist/page.js';
import { Recording, RecordingError } from '../dist/recording.js';
import { ServerOperationError, TransportError } from '../dist/errors.js';
import { SidecarTransport } from '../dist/transport.js';

const id = 'bb4bb8d9-5e7b-4f21-8c3e-50d0722272b1';
const path = resolve('capture.mp4');
const artifact = { path, bytes: 1024, mimeType: 'video/mp4', container: 'mp4' };
const record = (overrides = {}) => ({ recordingId: id, pageId: 'tab', path, state: 'running', ...overrides });
const completed = record({ state: 'completed', artifact });
const client = (invoke, capabilities = ['screenshotOptions', 'screenshotFile', 'nativeRecording']) => ({
  supports: () => true,
  hasCapability: name => capabilities.includes(name),
  invoke,
});

test('capture requires advertised methods and capabilities before dispatch', async () => {
  for (const methodMissing of [false, true]) {
    let sent = 0;
    const peer = client(async () => { sent++; }, []);
    if (methodMissing) peer.supports = () => false;
    const page = new Page(peer, 'tab');
    for (const operation of [() => page.screenshot(), () => page.screenshotToFile({ path: 'page.png' }), () => page.record({ path: 'capture.mp4', recordingId: id })]) {
      await assert.rejects(operation(), error => {
        assert.equal(error.code, methodMissing ? 'UnsupportedMethod' : 'UnsupportedCapability');
        assert.equal(error.requestState, 'not_sent');
        return true;
      });
    }
    assert.equal(sent, 0);
  }
});

test('lost start response retains caller-known ID and path for browser recovery without restart', async () => {
  let admittedId;
  const calls = [];
  const lost = new TransportError('DeadlineExceeded', 'Admission response was lost', { requestId: 7, requestState: 'may_have_been_sent', stderr: 'peer diagnostic' });
  const peer = client(async (method, params) => {
    calls.push(method);
    if (method === 'recording.start') {
      admittedId = params.recordingId;
      throw lost;
    }
    assert.equal(params.recordingId, admittedId);
    return record({ recordingId: admittedId });
  });
  let failure;
  await assert.rejects(new Page(peer, 'tab').record({ path: 'capture.mp4' }), error => {
    failure = error;
    assert.ok(error instanceof RecordingError);
    assert.equal(error.recordingId, admittedId);
    assert.equal(error.path, path);
    assert.equal(error.requestState, 'may_have_been_sent');
    assert.equal(error.requestId, 7);
    assert.equal(error.stderr, 'peer diagnostic');
    assert.equal(error.cause, lost);
    return true;
  });
  const recovered = Browser.prototype.recording.call(peer, failure.recordingId);
  assert.equal((await recovered.status()).recordingId, admittedId);
  assert.deepEqual(calls, ['recording.start', 'recording.get']);
});

test('unsupported start and failed finalization retain canonical record and partial evidence', async () => {
  const failure = { stage: 'start', code: 'RecordingUnsupported', message: 'Encoder unavailable', partialPath: `${path}.partial`, cleanupUncertain: true };
  const failed = record({ state: 'failed', failure });
  const wire = new ServerOperationError({ code: 'RecordingUnsupported', message: failure.message, data: { recordingId: id, recording: failed, path, partialPath: failure.partialPath } }, { requestState: 'acknowledged' });
  await assert.rejects(new Page(client(async () => { throw wire; }), 'tab').record({ path, recordingId: id }), error => {
    assert.equal(error.code, 'RecordingUnsupported');
    assert.equal(error.recordingId, id);
    assert.equal(error.record, failed);
    assert.equal(error.failure, failure);
    assert.equal(error.partialPath, failure.partialPath);
    assert.equal(error.cleanupUncertain, true);
    return true;
  });
  const finalization = record({ state: 'failed', failure: { ...failure, stage: 'write', code: 'FileOutput', message: 'Disk quota exceeded' } });
  const recording = new Recording(client(async () => finalization), id, record());
  await assert.rejects(recording.stop(), error => error.record === finalization && error.failure.stage === 'write' && error.partialPath === failure.partialPath);
  assert.equal(recording.record, finalization);
  assert.equal(await recording.status(), finalization);
});

test('stop fulfills only a completed native artifact and cancel reports terminal races truthfully', async () => {
  for (const state of ['running', 'finalizing', 'failed', 'cancelled']) {
    const terminal = record({ state });
    const recording = new Recording(client(async () => terminal), id, record());
    await assert.rejects(recording.stop(), error => error instanceof RecordingError && error.record === terminal);
  }
  const recording = new Recording(client(async () => completed), id, record());
  assert.equal(await recording.stop(), artifact);
  assert.equal(await recording.stop(), artifact);
  assert.equal(await recording.cancel(), completed);
  const cancelled = record({ state: 'cancelled' });
  assert.equal(await new Recording(client(async () => cancelled), id).cancel(), cancelled);
  await assert.rejects(new Recording(client(async () => record({ state: 'finalizing' })), id).cancel(), error => error.code === 'Protocol');
});

test('record identity and published native artifact are checked rather than trusting a success response', async () => {
  for (const value of [record({ recordingId: 'different' }), record({ pageId: 'other' }), record({ path: resolve('other.mp4') }), record({ state: 'completed' }), record({ state: 'completed', artifact: { ...artifact, path: resolve('other.mp4') } }), record({ state: 'completed', artifact: { ...artifact, container: undefined } }), record({ state: 'completed', artifact: { ...artifact, bytes: Number.MAX_SAFE_INTEGER + 1 } })]) {
    await assert.rejects(new Recording(client(async () => value), id, record()).stop(), error => error.code === 'Protocol' && error.record === value);
  }
  let sent = 0;
  await assert.rejects(new Page(client(async () => { sent++; }), 'tab').record({ path, recordingId: 'not-a-uuid' }), error => error.recordingId === 'not-a-uuid' && error.requestState === 'not_sent');
  assert.equal(sent, 0);
});

for (const [name, received] of [
  ['recording ID', record({ recordingId: '71ba5d8f-ea5a-44cc-b04b-0627b68a49cb' })],
  ['page ID', record({ pageId: 'other' })],
  ['output path', record({ path: resolve('other.mp4') })],
  ['missing completed artifact', record({ state: 'completed' })],
  ['artifact output path', record({ state: 'completed', artifact: { ...artifact, path: resolve('other.mp4') } })],
]) {
  test(`error-carried recording evidence rejects inconsistent ${name} without replacing known identity`, async () => {
    const data = { recordingId: received.recordingId, recording: received, path: received.path };
    const wire = new ServerOperationError({ code: 'RecordingFailed', message: 'Recording failed', data }, { requestId: 9, requestState: 'acknowledged', stderr: 'peer diagnostic' });
    const peer = client(async () => { throw wire; });
    const initial = record();
    const recording = new Recording(peer, id, initial);
    const rejected = error => {
      assert.ok(error instanceof RecordingError);
      assert.equal(error.code, 'Protocol');
      assert.equal(error.recordingId, id);
      assert.equal(error.record, received);
      assert.equal(error.cause, wire);
      assert.equal(error.data, data);
      assert.equal(error.requestId, 9);
      assert.equal(error.requestState, 'acknowledged');
      assert.equal(error.stderr, 'peer diagnostic');
      return true;
    };
    await assert.rejects(recording.status(), rejected);
    assert.equal(recording.record, initial);
    await assert.rejects(new Page(peer, 'tab').record({ path, recordingId: id }), rejected);
  });
}

test('independent stop waiter abort and deadline do not cancel remote finalization', async t => {
  const transport = await SidecarTransport.start({
    serverPath: fileURLToPath(new URL('../fixtures/wire/transport-peer.mjs', import.meta.url)),
    env: { ...process.env, EOKA_TRANSPORT_TEST_MODE: 'recording' },
    timeoutMs: 3000,
    shutdownTimeoutMs: 1000,
  });
  t.after(() => transport.close());
  const recording = await new Page(transport, 'tab').record({ path, recordingId: id });
  const controller = new AbortController();
  const aborted = recording.stop({ signal: controller.signal, timeoutMs: 1000 });
  const expired = recording.stop({ timeoutMs: 20 });
  const surviving = recording.stop({ timeoutMs: 1000 });
  const rejection = assert.rejects(aborted, error => error instanceof RecordingError && error.code === 'Aborted' && error.recordingId === id);
  const deadline = assert.rejects(expired, error => error instanceof RecordingError && error.code === 'DeadlineExceeded' && error.recordingId === id);
  controller.abort();
  await Promise.all([rejection, deadline]);
  assert.deepEqual(await surviving, artifact);
  assert.equal((await recording.status()).state, 'completed');
  assert.deepEqual(await recording.stop(), artifact);
});

test('lost stop responses can be inspected and retried by the existing recording ID', async () => {
  const calls = [];
  let lost = true;
  const recording = new Recording(client(async (method, params) => {
    calls.push(method);
    assert.deepEqual(params, { recordingId: id });
    if (method === 'recording.stop' && lost) {
      lost = false;
      throw new TransportError('DeadlineExceeded', 'Finalization response was lost', { requestState: 'may_have_been_sent' });
    }
    return completed;
  }), id, record());
  await assert.rejects(recording.stop(), error => error.recordingId === id && error.path === path && error.record.state === 'running');
  assert.equal(await recording.status(), completed);
  assert.equal(await recording.stop(), artifact);
  assert.deepEqual(calls, ['recording.stop', 'recording.get', 'recording.stop']);
});

test('server-side failure evidence updates the handle and late status cannot erase terminal evidence', async () => {
  const failed = record({ state: 'failed', failure: { stage: 'disconnect', code: 'RecordingFailed', message: 'Target disappeared', partialPath: `${path}.partial`, cleanupUncertain: true } });
  const recording = new Recording(client(async () => {
    throw new ServerOperationError({ code: 'RecordingFailed', message: 'Target disappeared', data: { recordingId: id, recording: failed } }, { requestState: 'acknowledged' });
  }), id, record());
  await assert.rejects(recording.status(), error => error.record === failed && error.cleanupUncertain === true);
  assert.equal(recording.record, failed);
  let respond;
  const peer = client(async method => {
    if (method === 'recording.get') return new Promise(resolve => { respond = resolve; });
    return completed;
  });
  const concurrent = new Recording(peer, id, record());
  const staleStatus = concurrent.status();
  await concurrent.stop();
  respond(record({ state: 'finalizing' }));
  assert.equal((await staleStatus).state, 'finalizing');
  assert.equal(concurrent.record, completed);
});

test('failed recording handles retain late partial and publication evidence from the same terminal state', async () => {
  const failed = record({ state: 'failed', failure: { stage: 'disconnect', code: 'RecordingFailed', message: 'Target disappeared', cleanupUncertain: true } });
  const partial = record({ ...failed, failure: { ...failed.failure, partialPath: `${path}.partial` } });
  const publication = record({ ...partial, failure: { ...partial.failure, stage: 'publish', code: 'FileOutput', message: 'Late publication failed', cleanupUncertain: false } });
  let calls = 0;
  const recording = new Recording(client(async () => {
    if (calls++ === 0) return partial;
    throw new ServerOperationError({ code: 'FileOutput', message: publication.failure.message, data: { recordingId: id, recording: publication } }, { requestState: 'acknowledged' });
  }), id, failed);
  assert.equal(await recording.status(), partial);
  assert.equal(recording.record, partial);
  await assert.rejects(recording.status(), error => {
    assert.equal(error.record, publication);
    assert.equal(error.partialPath, `${path}.partial`);
    assert.equal(error.failure.stage, 'publish');
    assert.equal(error.cleanupUncertain, false);
    return true;
  });
  assert.equal(recording.record, publication);
});

test('unknown recovery IDs remain explicit failures rather than creating a recording', async () => {
  const calls = [];
  const recording = new Recording(client(async method => {
    calls.push(method);
    throw new ServerOperationError({ code: 'RecordingNotFound', message: 'Unknown recording ID', data: { recordingId: id } }, { requestState: 'acknowledged' });
  }), id);
  await assert.rejects(recording.status(), error => error instanceof RecordingError && error.code === 'RecordingNotFound' && error.recordingId === id && error.record === undefined);
  assert.deepEqual(calls, ['recording.get']);
});
