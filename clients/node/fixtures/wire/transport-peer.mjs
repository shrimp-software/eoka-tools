#!/usr/bin/env node
import { createInterface } from 'node:readline';
import { spawn } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { methodNames, protocolVersion } from '../../dist/generated/protocol.js';

const mode = process.env.EOKA_TRANSPORT_TEST_MODE ?? 'normal';
const maxInFlight = Number(process.env.EOKA_TRANSPORT_TEST_IN_FLIGHT ?? 128);
const maxMessageBytes = Number(process.env.EOKA_TRANSPORT_TEST_MESSAGE_BYTES ?? 33554432);
const input = createInterface({ input: process.stdin, crlfDelay: Infinity });
let evaluations = 0;
let pair = [];
let terminalReaderClosed = false;
let recording;
let finalization;
const ownsDescendant = mode === 'owned-descendant' || mode === 'owned-descendant-exiting';
let ownedDescendantPid;
let ownedReady = Promise.resolve();
if (ownsDescendant) {
  const descendant = spawn(process.execPath, [fileURLToPath(new URL('./transport-descendant.mjs', import.meta.url))], { stdio: ['ignore', 'ignore', 'ignore', 'ipc'] });
  ownedReady = new Promise((resolve, reject) => {
    descendant.once('message', message => {
      ownedDescendantPid = message.pid;
      resolve();
    });
    descendant.once('error', reject);
  });
}

function reply(id, result) {
  const frame = `${JSON.stringify({ id, result })}\n`;
  process.stdout.write(mode === 'hello-malformed' && id === 1 ? `${frame}{broken}\n` : frame);
}

function receipt(overrides = {}) {
  return {
    action: 'click', already_satisfied: false, cleanup_pending: false,
    dispatch: 'acknowledged', elapsed_ms: 1, finished_at_ms: 2,
    outcome: 'verified_complete', preparation_may_have_executed: true,
    started_at_ms: 1, target_id: 'page', verification: 'url', ...overrides,
  };
}

if (mode === 'stubborn' || mode === 'owned-descendant') process.on('SIGTERM', () => {});
input.on('close', () => {
  if (terminalReaderClosed) return;
  if (mode !== 'stubborn' && !ownsDescendant) process.stdout.end(() => process.exit(0));
});
if (mode === 'stubborn' || ownsDescendant) setInterval(() => {}, 1000);

input.on('line', async line => {
  const request = JSON.parse(line);
  if (request.method === 'server.hello') {
    if (mode === 'handshake-hang') return;
    await ownedReady;
    reply(request.id, {
      protocol: mode === 'incompatible-protocol' ? { ...protocolVersion, major: protocolVersion.major + 1 } : protocolVersion, serverVersion: 'transport-fixture', coreVersion: 'fixture',
      methods: mode === 'unsupported' ? ['server.hello', 'server.shutdown'] : methodNames,
      capabilities: mode === 'recording' ? ['fixture', 'nativeRecording'] : ['fixture'], limits: { maxInFlight: mode === 'invalid-limits' ? 0 : maxInFlight, maxMessageBytes },
      ...(ownedDescendantPid === undefined ? {} : { ownedDescendantPid }),
    });
    if (mode === 'backpressure') {
      input.pause();
      setTimeout(() => input.resume(), 250);
    }
    return;
  }
  if ((request.method === 'server.shutdown' || request.method === 'browser.close') && mode === 'terminal-malformed') {
    process.stdout.write('{broken}\n');
    return;
  }
  if ((request.method === 'server.shutdown' || request.method === 'browser.close') && (mode === 'terminal-reader-closed' || mode === 'terminal-eof-no-reply')) {
    terminalReaderClosed = true;
    input.close();
    process.stdin.destroy();
    if (mode === 'terminal-reader-closed') reply(request.id, {});
    setTimeout(() => process.stdout.end(() => process.exit(0)), 30);
    return;
  }
  if (request.method === 'browser.close') {
    if (mode === 'terminal-browser-error') process.stdout.write(`${JSON.stringify({ id: request.id, error: { code: 'Internal', message: 'Browser cleanup failed', data: { detail: { code: 'BrowserCleanup', message: 'Browser cleanup failed' } } } })}\n`);
    else reply(request.id, {});
    return;
  }
  if (request.method === 'server.shutdown') {
    if (mode === 'stubborn' || ownsDescendant) return;
    if (mode === 'shutdown-error') {
      process.stdout.write(`${JSON.stringify({ id: request.id, error: { code: 'Internal', message: 'Cleanup failed', data: { detail: { code: 'Cleanup', message: 'Profile could not be released' } } } })}\n`);
    } else reply(request.id, {});
    return;
  }
  if (request.method === 'browser.close_tab') {
    reply(request.id, {});
    return;
  }
  if (mode === 'recording') {
    if (request.method === 'recording.start') {
      recording = { recordingId: request.params.recordingId, pageId: request.params.pageId, path: request.params.path, state: 'running' };
      reply(request.id, recording);
    } else if (request.method === 'recording.get') {
      reply(request.id, recording);
    } else if (request.method === 'recording.cancel') {
      recording = { ...recording, state: 'cancelled' };
      reply(request.id, recording);
    } else if (request.method === 'recording.stop') {
      if (finalization === undefined) {
        recording = { ...recording, state: 'finalizing' };
        finalization = new Promise(resolve => setTimeout(() => {
          if (recording.state !== 'cancelled') {
            recording = { ...recording, state: 'completed', artifact: { path: recording.path, bytes: 1024, mimeType: 'video/mp4', container: 'mp4' } };
          }
          resolve();
        }, 100));
      }
      await finalization;
      reply(request.id, recording);
    }
    return;
  }
  evaluations++;
  if (mode === 'malformed') process.stdout.write('{broken}\n');
  else if (mode === 'empty-frame') process.stdout.write('\n');
  else if (mode === 'empty-crlf') process.stdout.write('\r\n');
  else if (mode.startsWith('boundary-') && request.method === 'page.evaluate') {
    const overhead = Buffer.byteLength(JSON.stringify({ id: request.id, result: { result: '' } }));
    const result = 'x'.repeat(maxMessageBytes + Number(mode.includes('oversized')) - overhead);
    const frame = Buffer.from(`${JSON.stringify({ id: request.id, result: { result } })}${mode.includes('crlf') ? '\r\n' : '\n'}`);
    if (mode.includes('split')) {
      process.stdout.write(frame.subarray(0, frame.length - 1));
      setTimeout(() => process.stdout.write(frame.subarray(frame.length - 1)), 10);
    } else process.stdout.write(frame);
  }
  else if (mode === 'array-envelope') process.stdout.write('[]\n');
  else if (mode === 'dual-envelope') process.stdout.write(`${JSON.stringify({ id: request.id, result: {}, error: { code: 'Internal', message: 'bad' } })}\n`);
  else if (mode === 'invalid-utf8') process.stdout.write(Buffer.from([123, 34, 120, 34, 58, 34, 0xc3, 0x28, 34, 125, 10]));
  else if (mode === 'oversized') process.stdout.write(Buffer.alloc(maxMessageBytes + 2, 120));
  else if (mode === 'unknown-id') reply(request.id + 10, {});
  else if (mode === 'null-error') process.stdout.write(`${JSON.stringify({ id: null, error: { code: 'InvalidParams', message: 'Invalid frame' } })}\n`);
  else if (mode === 'eof') process.stdout.end();
  else if (mode === 'partial-eof') process.stdout.end('{"id":');
  else if (mode === 'crash') process.stderr.write('x'.repeat(32768) + '\ncrash-tail\n', () => setTimeout(() => process.exit(23), 20));
  else if (mode === 'secret-stderr' || mode === 'secret-stderr-clipped') {
    const chunks = [
      'x'.repeat(32768) + '\n',
      'eoka-server: error while loading shared libraries: libX.so: not found\n',
      'Chrome sandbox failed: permission denied\n',
      'https://url-user:url-pass',
      'word@example.test/path?url-query-secret=secret#url-fragment-secret\n',
      'Authoriza', 'tion: Bearer bearer-secret\n',
      'token=token-secret password=password-secret\n',
      '{"authorization":"json-auth-secret","password":"json-password-secret"}\n',
      'configured-proxy-secret-without-any-label\n',
      'DevTools listening on ws://127.0.0.1:9222/devtools/browser/devtools-secret\n',
      'Cookie: session=cookie-secret\nAPI-Key: api-key-secret\n',
      'EOKA_API_KEY=environment-api-secret\nX-Auth-Token: header-token-secret\n--password "quoted secret value"\n',
      mode === 'secret-stderr-clipped' ? 'partial-leading-secret'.repeat(2048) + '\nbenign final line\npartial-trailing-secret' : 'benign final line\npartial-trailing-secret',
    ];
    for (const chunk of chunks) {
      process.stderr.write(chunk);
      await new Promise(resolve => setTimeout(resolve, 2));
    }
    setTimeout(() => process.exit(23), 20);
  }
  else if (mode === 'late') setTimeout(() => reply(request.id, { result: null }), 80);
  else if (mode === 'backpressure') reply(request.id, { result: { index: evaluations } });
  else if (mode === 'invalid-result') reply(request.id, {});
  else if (mode === 'operation-error') process.stdout.write(`${JSON.stringify({ id: request.id, error: { code: 'Action', message: 'Unknown outcome', data: { operation_id: request.params.operation_id, receipt: receipt({ outcome: 'outcome_unknown', dispatch: 'may_have_been_sent', verification: null }) } } })}\n`);
  else if (mode === 'invalid-operation') reply(request.id, {
    id: request.params.id, action: 'click', target_id: 'page', runtime_id: 'runtime',
    started_at_ms: 1, finished_at_ms: 2, dispatch_intent: true, preparation_intent: true,
    outcome: 'verified_complete', state: 'finished', receipt: receipt({ verification: null }),
  });
  else if (mode === 'out-of-order' && request.method === 'page.evaluate') {
    pair.push(request);
    if (pair.length === 2) {
      for (const item of pair.reverse()) reply(item.id, { result: JSON.parse(item.params.js) });
      pair = [];
    }
  } else if (mode === 'split-utf8') {
    const bytes = Buffer.from(`${JSON.stringify({ id: request.id, result: { result: 'é🙂漢字' } })}\r\n`);
    for (const byte of bytes) {
      process.stdout.write(Buffer.from([byte]));
      await new Promise(resolve => setTimeout(resolve, 1));
    }
  } else if (request.method === 'operations.list') reply(request.id, []);
  else reply(request.id, { result: JSON.parse(request.params.js) });
});
