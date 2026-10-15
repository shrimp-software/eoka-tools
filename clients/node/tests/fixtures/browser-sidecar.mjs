import { appendFileSync, existsSync, mkdirSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { execFileSync } from 'node:child_process';
import { createInterface } from 'node:readline';
import { methodNames, protocolVersion } from '../../dist/generated/protocol.js';

const mode = process.env.EOKA_BROWSER_FIXTURE_MODE;
const logPath = process.env.EOKA_BROWSER_FIXTURE_LOG;
let browserVersion = process.env.EOKA_BROWSER_FIXTURE_VERSION ?? 'Chrome/fixture';
const log = event => appendFileSync(logPath, `${JSON.stringify(event)}\n`);
const reply = (id, result) => process.stdout.write(`${JSON.stringify({ id, result })}\n`);
const fail = (id, message) => process.stdout.write(`${JSON.stringify({ id, error: { code: 'Internal', message } })}\n`);
let borrowed = false;
let record;
let closing = false;
log({ event: 'spawn', pid: process.pid, temp: { TMPDIR: process.env.TMPDIR, TMP: process.env.TMP, TEMP: process.env.TEMP } });
if (mode === 'stubborn-cleanup' || mode === 'failed-session-stubborn') {
  setInterval(() => {}, 1000);
}
let runtimeDir;
process.on('exit', () => log({ event: 'exit', runtimeExists: runtimeDir !== undefined && existsSync(runtimeDir) }));

const input = createInterface({ input: process.stdin });
input.on('close', () => {
  if (!closing) log({ event: 'eof-cleanup', ownership: borrowed ? 'borrowed' : 'owned' });
});
input.on('line', line => {
  const request = JSON.parse(line);
  log({ event: 'request', ...request });
  const { id, method, params } = request;
  switch (method) {
    case 'server.hello':
      setTimeout(() => reply(id, {
        protocol: protocolVersion,
        serverVersion: process.env.EOKA_BROWSER_FIXTURE_SERVER_VERSION ?? 'lifecycle-fixture',
        coreVersion: 'lifecycle-fixture',
        capabilities: ['lifecycleFixture'],
        methods: methodNames.filter(name => name !== 'page.execute' && (mode !== 'no-connect' || name !== 'browser.connect')),
        limits: { maxInFlight: 8, maxMessageBytes: 1048576 },
      }), mode === 'managed-deadline' ? 1000 : mode === 'shared-deadline' ? 400 : 0);
      break;
    case 'browser.launch':
    case 'browser.connect':
      if (method === 'browser.launch') {
        runtimeDir = params.runtimeDir;
        mkdirSync(join(runtimeDir, 'core-owned'), { recursive: true });
        writeFileSync(join(runtimeDir, 'core-owned', 'scratch'), 'owned scratch');
        if (process.env.EOKA_BROWSER_FIXTURE_EXECUTE_CHROME === '1') {
          const actualVersion = execFileSync(params.executablePath, ['--version'], { encoding: 'utf8' }).trim();
          log({ event: 'browser-executable', executablePath: params.executablePath, actualVersion });
          browserVersion = process.env.EOKA_BROWSER_FIXTURE_VERSION ?? actualVersion;
        }
      }
      borrowed = method === 'browser.connect' || mode === 'borrowed-launch';
      if (mode === 'pending-session') break;
      if (mode === 'failed-session' || mode === 'failed-session-cleanup' || mode === 'failed-session-stubborn') {
        fail(id, 'session initialization failed');
      } else if (mode === 'bad-session') {
        reply(id, { ownership: borrowed ? 'borrowed' : 'owned', browserVersion: ' ' });
      } else if (mode === 'shared-deadline' || mode === 'managed-deadline') {
        setTimeout(() => reply(id, { ownership: 'owned', browserVersion }), mode === 'managed-deadline' ? 2000 : 700);
      } else {
        reply(id, { ownership: mode === 'owned-connect' ? 'owned' : borrowed ? 'borrowed' : 'owned', browserVersion });
        if (mode === 'spontaneous-exit') setTimeout(() => process.exit(0), 120);
      }
      break;
    case 'browser.new_page':
      if (mode === 'pending-creation') break;
      setTimeout(() => {
        log({ event: 'tab-created', pageId: 'created-tab' });
        reply(id, { pageId: 'created-tab' });
      }, mode === 'delayed-creation' ? 150 : 0);
      break;
    case 'browser.tabs':
      reply(id, { tabs: [{ id: 'existing-tab', url: 'about:blank', title: 'Existing' }] });
      break;
    case 'actions.perform':
      record = {
        id: params.operation_id,
        runtime_id: 'browser-lifecycle-fixture',
        target_id: params.pageId,
        action: params.action.kind,
        state: 'running',
        outcome: 'outcome_unknown',
        preparation_intent: false,
        dispatch_intent: true,
        started_at_ms: 1,
      };
      reply(id, record);
      break;
    case 'operations.wait':
      if (mode === 'pending-wait' || mode === 'abort-navigation' || mode === 'borrowed-launch') break;
      setTimeout(() => {
        const outcome = mode === 'unknown-navigation' || mode === 'navigation-cleanup-failure' ? 'outcome_unknown' : 'verified_complete';
        record = {
          ...record,
          state: 'finished',
          outcome,
          finished_at_ms: 2,
          receipt: {
            action: record.action,
            already_satisfied: false,
            cleanup_pending: false,
            dispatch: 'acknowledged',
            elapsed_ms: 1,
            finished_at_ms: 2,
            outcome,
            preparation_may_have_executed: false,
            started_at_ms: 1,
            target_id: 'created-tab',
            verification: outcome === 'verified_complete' ? 'URL matched' : null,
          },
        };
        log({ event: 'verified', outcome });
        reply(id, record);
      }, 80);
      break;
    case 'operations.get':
      reply(id, record);
      break;
    case 'operations.list':
      reply(id, record ? [record] : []);
      break;
    case 'browser.close_tab':
      setTimeout(() => {
        log({ event: 'tab-closed', pageId: params.pageId });
        reply(id, mode === 'navigation-cleanup-failure' ? { cleanupError: 'input release failed' } : {});
      }, 40);
      break;
    case 'server.shutdown':
    case 'browser.close':
      closing = true;
      if (mode === 'stubborn-cleanup' || mode === 'failed-session-stubborn') break;
      setTimeout(() => {
        log({ event: 'cleanup', ownership: borrowed ? 'borrowed' : 'owned' });
        if (mode === 'failed-session-cleanup') fail(id, 'shutdown cleanup failed');
        else reply(id, {});
        input.close();
        process.stdin.destroy();
        if (mode === 'reader-closed-first') {
          log({ event: 'reader-closed' });
          setTimeout(() => log({ event: 'exit-after-reader-close' }), 120);
        }
      }, 100);
      break;
    default:
      fail(id, `Unexpected fixture method ${method}`);
  }
});
