import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { once } from 'node:events';
import { createServer } from 'node:http';
import { setTimeout as delay } from 'node:timers/promises';
import { Browser } from '../../dist/index.js';

export const skipRealBrowser = !process.env.EOKA_RELIABILITY_SERVER;

export function launchOptions() {
  assert.ok(process.env.EOKA_RELIABILITY_CHROME);
  return {
    serverPath: process.env.EOKA_RELIABILITY_SERVER,
    executablePath: process.env.EOKA_RELIABILITY_CHROME,
    headless: true,
    timeoutMs: 30000,
    shutdownTimeoutMs: 10000,
  };
}

export function launchBrowser() {
  return Browser.launch(launchOptions());
}

const fixtureDocument = `<!doctype html><title>fixture</title>
<input id="name">
<input id="ro" readonly value="locked">
<button id="off" disabled>Off</button>
<p id="status"></p>
<button id="real" onclick="fetch('/hit'+location.search,{method:'POST'});document.getElementById('status').textContent='Saved'">Go</button>`;

export async function startFixture() {
  const clicks = new Map();
  const documents = new Map();
  const server = createServer((request, response) => {
    const url = new URL(request.url, 'http://fixture.invalid');
    const row = url.searchParams.get('row') ?? '';
    if (url.pathname === '/hang') return;
    if (url.pathname === '/hit') {
      clicks.set(row, (clicks.get(row) ?? 0) + 1);
      response.writeHead(204).end();
      return;
    }
    if (url.pathname === '/') documents.set(row, (documents.get(row) ?? 0) + 1);
    response.writeHead(200, { 'content-type': 'text/html' }).end(fixtureDocument);
  });
  server.listen(0, '127.0.0.1');
  await once(server, 'listening');
  const origin = `http://127.0.0.1:${server.address().port}`;
  return {
    origin,
    hangUrl: `${origin}/hang`,
    pageUrl: (row = '') => `${origin}/?row=${encodeURIComponent(row)}`,
    clicks: (row = '') => clicks.get(row) ?? 0,
    documents: (row = '') => documents.get(row) ?? 0,
    async close() {
      server.closeAllConnections();
      await new Promise(resolve => server.close(resolve));
    },
  };
}

export async function settle(call, budgetMs) {
  const started = performance.now();
  let timer;
  const settled = Promise.resolve().then(call).then(
    value => ({ state: 'resolved', value }),
    error => ({ state: 'rejected', error }),
  );
  const outcome = await Promise.race([
    settled,
    new Promise(resolve => { timer = setTimeout(resolve, budgetMs, { state: 'pending' }); }),
  ]);
  clearTimeout(timer);
  return { ...outcome, ms: Math.round(performance.now() - started), budgetMs, settled };
}

function preview(value) {
  const text = typeof value === 'string' ? value : JSON.stringify(value) ?? String(value);
  return text.length > 300 ? `${text.slice(0, 300)}…` : text;
}

export function describeOutcome(outcome) {
  if (outcome.state === 'pending') return `still pending after ${outcome.budgetMs} ms`;
  if (outcome.state === 'resolved') return `resolved in ${outcome.ms} ms with ${preview(outcome.value)}`;
  const { error } = outcome;
  return `rejected in ${outcome.ms} ms with ${error?.constructor?.name} code=${error?.code} requestState=${error?.requestState}: ${preview(error?.message)}`;
}

export function rejection(outcome) {
  assert.equal(outcome.state, 'rejected', `expected a rejection within ${outcome.budgetMs} ms; ${describeOutcome(outcome)}`);
  return outcome.error;
}

export function resolution(outcome) {
  assert.equal(outcome.state, 'resolved', `expected a result within ${outcome.budgetMs} ms; ${describeOutcome(outcome)}`);
  return outcome.value;
}

export function callerFrames(error) {
  return String(error?.stack).split('\n').filter(line => /^\s+at /.test(line)).join('\n');
}

export async function closeQuietly(...browsers) {
  await Promise.allSettled(browsers.map(browser => browser?.close()));
}

export function processTable() {
  const output = execFileSync('ps', ['-A', '-o', 'pid=,ppid=,stat=,comm='], { encoding: 'utf8' });
  const table = new Map();
  for (const line of output.split('\n')) {
    const match = /^\s*(\d+)\s+(\d+)\s+(\S+)\s+(.*)$/.exec(line);
    if (match) table.set(Number(match[1]), { parent: Number(match[2]), zombie: match[3].startsWith('Z'), command: match[4].trim() });
  }
  return table;
}

export function descendants(root, table = processTable()) {
  const found = [];
  const queue = [root];
  while (queue.length) {
    const parent = queue.shift();
    for (const [pid, entry] of table) {
      if (entry.parent === parent && !entry.zombie) {
        found.push({ pid, command: entry.command, direct: parent === root });
        queue.push(pid);
      }
    }
  }
  return found;
}

export function survivors(processes) {
  const table = processTable();
  return processes.filter(({ pid }) => table.has(pid) && !table.get(pid).zombie);
}

export async function goneWithin(processes, budgetMs) {
  const started = performance.now();
  let remaining = survivors(processes);
  while (remaining.length && performance.now() - started < budgetMs) {
    await delay(50);
    remaining = survivors(processes);
  }
  return { remaining, ms: Math.round(performance.now() - started) };
}

export function killSurvivors(processes) {
  for (const { pid } of survivors(processes)) {
    try {
      process.kill(pid, 'SIGKILL');
    } catch {}
  }
}
