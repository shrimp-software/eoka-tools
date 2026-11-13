import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { once } from 'node:events';
import { after, before, describe, test } from 'node:test';
import { setTimeout as delay } from 'node:timers/promises';
import { fileURLToPath } from 'node:url';
import { EokaError } from '../dist/index.js';
import {
  describeOutcome, descendants, goneWithin, killSurvivors, launchBrowser, rejection, resolution, settle, skipRealBrowser, startFixture,
} from './fixtures/eval-support.mjs';

const sleepOneSecond = '()=>new Promise(r=>setTimeout(()=>r(1),1000))';
const neverSettles = '()=>new Promise(()=>{})';
const fieldValue = "()=>document.querySelector('#name').value";
const processChild = fileURLToPath(new URL('./fixtures/eval-process-child.mjs', import.meta.url));

function within(outcome, limitMs, label) {
  resolution(outcome);
  assert.ok(outcome.ms < limitMs, `expected ${label} in under ${limitMs} ms; ${describeOutcome(outcome)}`);
}

async function closeQuietly(browser) {
  try {
    await browser?.close();
  } catch {}
}

describe('healthy tabs side by side', { skip: skipRealBrowser }, () => {
  let fixture;
  let browser;
  let a;
  let b;

  before(async () => {
    fixture = await startFixture();
    browser = await launchBrowser();
    a = await browser.newPage(fixture.pageUrl('a'));
    b = await browser.newPage(fixture.pageUrl('b'));
  }, { timeout: 90000 });

  after(async () => {
    try {
      await browser?.close();
    } finally {
      await fixture?.close();
    }
  }, { timeout: 60000 });

  test('K1: two tabs that each wait one second finish together in under 1600 ms', { timeout: 60000 }, async t => {
    const visibility = await Promise.all([a.evaluate('()=>document.visibilityState'), b.evaluate('()=>document.visibilityState')]);
    const outcome = await settle(async () => await Promise.all([a.evaluate(sleepOneSecond), b.evaluate(sleepOneSecond)]), 15000);
    t.diagnostic(`${describeOutcome(outcome)}; tab visibility ${visibility.join(', ')}`);
    assert.deepEqual(resolution(outcome), [1, 1]);
    within(outcome, 1600, 'both evaluations');
  });

  test('K3: 300 concurrent title() calls on one page all resolve', { timeout: 60000 }, async t => {
    const outcome = await settle(async () => await Promise.allSettled(Array.from({ length: 300 }, () => a.title())), 30000);
    const settled = resolution(outcome);
    const rejected = settled.filter(result => result.status === 'rejected');
    const codes = [...new Set(rejected.map(result => result.reason?.code))];
    t.diagnostic(`${settled.length - rejected.length} resolved, ${rejected.length} rejected${rejected.length ? ` with ${codes.join(', ')}` : ''} in ${outcome.ms} ms`);
    assert.equal(rejected.length, 0, `expected no rejection; ${rejected.length} of ${settled.length} rejected with ${codes.join(', ')}: ${rejected[0]?.reason?.message}`);
    assert.deepEqual([...new Set(settled.map(result => result.value))], ['fixture']);
  });

  test('pinned: typing into two tabs in parallel does not cross-contaminate', { timeout: 60000 }, async t => {
    const left = 'left-'.repeat(8);
    const right = 'right'.repeat(8);
    const outcome = await settle(async () => await Promise.all([a.fill('#name', left), b.fill('#name', right)]), 30000);
    t.diagnostic(describeOutcome({ ...outcome, value: outcome.state === 'resolved' ? 'both fills' : undefined }));
    resolution(outcome);
    assert.deepEqual([await a.evaluate(fieldValue), await b.evaluate(fieldValue)], [left, right]);
  });
});

describe('a tab stuck in an evaluation that never settles', { skip: skipRealBrowser }, () => {
  let fixture;
  let browser;
  let b;
  let stuckSettled = false;
  const abandon = new AbortController();

  before(async () => {
    fixture = await startFixture();
    browser = await launchBrowser();
    const a = await browser.newPage(fixture.pageUrl('a'));
    b = await browser.newPage(fixture.pageUrl('b'));
    await b.title();
    a.evaluate(neverSettles, [], { signal: abandon.signal }).then(() => { stuckSettled = true; }, () => { stuckSettled = true; });
    await delay(250);
  }, { timeout: 90000 });

  after(async () => {
    abandon.abort();
    await closeQuietly(browser);
    await fixture?.close();
  }, { timeout: 60000 });

  test('K2: another tab answers title() in under 1000 ms', { timeout: 30000 }, async t => {
    assert.equal(stuckSettled, false, 'the stuck evaluation settled before the measurement');
    const outcome = await settle(async () => await b.title(), 1000);
    t.diagnostic(describeOutcome(outcome));
    assert.equal(resolution(outcome), 'fixture');
  });

  test('K2: browser.newPage() resolves in under 3000 ms', { timeout: 30000 }, async t => {
    assert.equal(stuckSettled, false, 'the stuck evaluation settled before the measurement');
    const outcome = await settle(async () => await browser.newPage(), 3000);
    t.diagnostic(describeOutcome({ ...outcome, value: outcome.value?.id }));
    resolution(outcome);
  });
});

describe('a tab stuck on a navigation that never answers', { skip: skipRealBrowser }, () => {
  let fixture;
  let browser;
  let a;
  let b;
  let navigation;
  let stuckTitle;

  function titleOnStuckTab() {
    stuckTitle ??= settle(async () => await a.title(), 35000);
    return stuckTitle;
  }

  before(async () => {
    fixture = await startFixture();
    browser = await launchBrowser();
    a = await browser.newPage(fixture.pageUrl('a'));
    b = await browser.newPage(fixture.pageUrl('b'));
    await b.title();
    navigation = await settle(async () => await a.goto(fixture.hangUrl, { timeoutMs: 3000 }), 15000);
  }, { timeout: 90000 });

  after(async () => {
    await closeQuietly(browser);
    await fixture?.close();
  }, { timeout: 60000 });

  test('K2 precondition: goto with timeoutMs 3000 to a server that never answers rejects', { timeout: 30000 }, t => {
    t.diagnostic(describeOutcome(navigation));
    assert.ok(rejection(navigation) instanceof EokaError);
  });

  test('pinned: right after that rejection another tab answers title() in under 1000 ms and newPage() in under 3000 ms', { timeout: 30000 }, async t => {
    rejection(navigation);
    const title = await settle(async () => await b.title(), 1000);
    const created = await settle(async () => await browser.newPage(), 3000);
    t.diagnostic(`title(): ${describeOutcome(title)}; newPage(): ${describeOutcome({ ...created, value: created.value?.id })}`);
    assert.equal(resolution(title), 'fixture');
    resolution(created);
  });

  test('K2: with a call on the stuck tab in flight another tab answers title() in under 1000 ms', { timeout: 30000 }, async t => {
    rejection(navigation);
    titleOnStuckTab();
    await delay(100);
    const outcome = await settle(async () => await b.title(), 1000);
    t.diagnostic(describeOutcome(outcome));
    assert.equal(resolution(outcome), 'fixture');
  });

  test('K2: with a call on the stuck tab in flight browser.newPage() resolves in under 3000 ms', { timeout: 30000 }, async t => {
    rejection(navigation);
    titleOnStuckTab();
    await delay(100);
    const outcome = await settle(async () => await browser.newPage(), 3000);
    t.diagnostic(describeOutcome({ ...outcome, value: outcome.value?.id }));
    resolution(outcome);
  });

  test('K2: title() on the stuck tab settles within 35 s, resolving or rejecting with an EokaError', { timeout: 60000 }, async t => {
    rejection(navigation);
    const outcome = await titleOnStuckTab();
    t.diagnostic(describeOutcome(outcome));
    assert.notEqual(outcome.state, 'pending', describeOutcome(outcome));
    if (outcome.state === 'rejected') assert.ok(outcome.error instanceof EokaError, describeOutcome(outcome));
  });
});

describe('K14: process hygiene', { skip: skipRealBrowser || process.platform === 'win32' }, () => {
  let fixture;

  before(async () => {
    fixture = await startFixture();
  });

  after(async () => {
    await fixture?.close();
  });

  async function startChild(mode) {
    const child = spawn(process.execPath, [processChild, mode, fixture.pageUrl(mode)], { stdio: ['pipe', 'pipe', 'pipe'] });
    const exited = once(child, 'exit');
    let stdout = '';
    let stderr = '';
    child.stderr.setEncoding('utf8').on('data', chunk => { stderr += chunk; });
    const ready = new Promise((resolve, reject) => {
      child.stdout.setEncoding('utf8').on('data', chunk => {
        stdout += chunk;
        if (stdout.includes('\n')) resolve(stdout);
      });
      exited.then(([code, signal]) => reject(new Error(`child exited before it was ready (code ${code}, signal ${signal}): ${stderr}`)));
    });
    const outcome = await settle(() => ready, 60000);
    assert.equal(resolution(outcome), 'ready fixture\n');
    const tree = descendants(child.pid);
    return { child, exited, tree, stderr: () => stderr };
  }

  function assertBrowserTree(tree) {
    const summary = tree.map(entry => `${entry.pid}:${entry.command}`).join(' ');
    assert.ok(tree.some(entry => entry.direct), `expected a sidecar child process; found ${summary}`);
    assert.ok(tree.some(entry => !entry.direct), `expected Chrome processes under the sidecar; found ${summary}`);
  }

  test('K14: after SIGKILL of the Node process the sidecar and Chrome are gone within 5 s', { timeout: 120000 }, async t => {
    const { child, exited, tree } = await startChild('hold');
    try {
      assertBrowserTree(tree);
      child.kill('SIGKILL');
      const { remaining, ms } = await goneWithin(tree, 5000);
      t.diagnostic(`${tree.length} sidecar and Chrome processes gone ${ms} ms after SIGKILL; ${remaining.length} left`);
      assert.deepEqual(remaining, [], `processes still alive ${ms} ms after SIGKILL`);
      await exited;
    } finally {
      child.kill('SIGKILL');
      killSurvivors(tree);
    }
  });

  test('K14: after an uncaught exception the process exits non-zero within 10 s leaving no child processes', { timeout: 120000 }, async t => {
    const { child, exited, tree, stderr } = await startChild('throw');
    try {
      assertBrowserTree(tree);
      child.stdin.write('throw\n');
      const exit = await settle(() => exited, 10000);
      const [code, signal] = resolution(exit);
      const { remaining, ms } = await goneWithin(tree, 5000);
      t.diagnostic(`exit code ${code}, signal ${signal} after ${exit.ms} ms; ${tree.length} sidecar and Chrome processes gone ${ms} ms after exit; ${remaining.length} left`);
      assert.notEqual(code, 0, `expected a non-zero exit; ${stderr()}`);
      assert.match(stderr(), /uncaught fixture failure/);
      assert.deepEqual(remaining, [], `processes still alive ${ms} ms after exit`);
    } finally {
      child.kill('SIGKILL');
      killSurvivors(tree);
    }
  });
});
