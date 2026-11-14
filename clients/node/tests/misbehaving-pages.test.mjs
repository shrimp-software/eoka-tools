import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { once } from 'node:events';
import { mkdtemp, rm, stat } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { after, before, describe, test } from 'node:test';
import { setTimeout as delay } from 'node:timers/promises';
import { fileURLToPath } from 'node:url';
import { Browser, EokaError } from '../dist/index.js';
import {
  closeQuietly, describeOutcome, descendants, goneWithin, killSurvivors, launchBrowser, launchOptions, rejection, resolution, settle, skipRealBrowser, startFixture,
} from './fixtures/eval-support.mjs';

const sleepOneSecond = '()=>new Promise(r=>setTimeout(()=>r(1),1000))';
const neverSettles = '()=>new Promise(()=>{})';
const fieldValue = "()=>document.querySelector('#name').value";
const processChild = fileURLToPath(new URL('./fixtures/eval-process-child.mjs', import.meta.url));

const pngSignature = Buffer.from([137, 80, 78, 71, 13, 10, 26, 10]);

function describeCapture(outcome) {
  return describeOutcome(outcome.state === 'resolved' ? { ...outcome, value: `${outcome.value?.length} bytes` } : outcome);
}

function png(outcome, label) {
  const image = resolution(outcome);
  assert.ok(Buffer.isBuffer(image) && image.length > pngSignature.length, `expected ${label} to return a non-empty image; got ${image?.length} bytes`);
  assert.deepEqual(image.subarray(0, pngSignature.length), pngSignature, `expected ${label} to return a PNG`);
}

function within(outcome, limitMs, label) {
  resolution(outcome);
  assert.ok(outcome.ms < limitMs, `expected ${label} in under ${limitMs} ms; ${describeOutcome(outcome)}`);
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
    await closeQuietly(browser);
    await fixture?.close();
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

describe('K15: capturing a tab that is not in front', { skip: skipRealBrowser, concurrency: true }, () => {
  const rows = ['screenshot', 'file', 'front', 'together'];
  const stages = new Map();
  let fixture;
  let directory;

  before(async () => {
    fixture = await startFixture();
    directory = await mkdtemp(join(tmpdir(), 'eoka-capture-'));
    const launched = await Promise.allSettled(rows.map(() => launchBrowser()));
    rows.forEach((row, index) => {
      if (launched[index].status === 'fulfilled') stages.set(row, { browser: launched[index].value });
    });
    const failed = launched.find(result => result.status === 'rejected');
    if (failed) throw failed.reason;
    await Promise.all(rows.map(async row => {
      const stage = stages.get(row);
      stage.a = await stage.browser.newPage(fixture.pageUrl(`${row}-a`));
      stage.b = await stage.browser.newPage(fixture.pageUrl(`${row}-b`));
      stage.visibility = [await stage.a.evaluate('()=>document.visibilityState'), await stage.b.evaluate('()=>document.visibilityState')];
    }));
    await delay(3000);
  }, { timeout: 120000 });

  after(async () => {
    await closeQuietly(...[...stages.values()].map(stage => stage.browser));
    await fixture?.close();
    if (directory) await rm(directory, { recursive: true, force: true });
  }, { timeout: 60000 });

  test('K15: screenshot() of a tab that has been behind for 3 s resolves within 5 s with a non-empty PNG', { timeout: 60000 }, async t => {
    const { a, visibility } = stages.get('screenshot');
    const outcome = await settle(async () => await a.screenshot(), 5000);
    t.diagnostic(`${describeCapture(outcome)}; tab visibility ${visibility.join(', ')}`);
    png(outcome, 'screenshot() of the tab behind');
  });

  test('K15: screenshotToFile() of a tab that has been behind for 3 s resolves within 5 s and writes a non-empty file', { timeout: 60000 }, async t => {
    const { a } = stages.get('file');
    const path = join(directory, 'behind.png');
    const outcome = await settle(async () => await a.screenshotToFile({ path }), 5000);
    const written = await stat(path).then(entry => entry.size, () => null);
    t.diagnostic(`${describeOutcome(outcome)}; file ${written === null ? 'missing' : `${written} bytes`}`);
    resolution(outcome);
    assert.ok(written > 0, `expected a non-empty file at ${path}; ${written === null ? 'it does not exist' : `it has ${written} bytes`}`);
  });

  test('K15: after capturing the tab behind, screenshot() of the front tab still resolves within 5 s', { timeout: 60000 }, async t => {
    const { a, b } = stages.get('front');
    const behind = await settle(async () => await a.screenshot(), 5000);
    const front = await settle(async () => await b.screenshot(), 5000);
    t.diagnostic(`tab behind: ${describeCapture(behind)}; front tab afterwards: ${describeCapture(front)}`);
    png(front, 'screenshot() of the front tab');
  });

  test('K15: screenshot() of the tab behind and of the front tab together both resolve within 10 s', { timeout: 60000 }, async t => {
    const { a, b } = stages.get('together');
    const [behind, front] = await Promise.all([settle(async () => await a.screenshot(), 10000), settle(async () => await b.screenshot(), 10000)]);
    t.diagnostic(`tab behind: ${describeCapture(behind)}; front tab: ${describeCapture(front)}`);
    png(behind, 'screenshot() of the tab behind');
    png(front, 'screenshot() of the front tab');
  });
});

describe('K16: acting on a tab that is not in front', { skip: skipRealBrowser, concurrency: true }, () => {
  const pointerDocument = `<!doctype html><title>fixture</title>
<input id="n">
<button id="b" onclick="document.getElementById('s').textContent='Saved'">Go</button>
<p id="s"></p>`;
  const saved = { kind: 'text', target: { query: { kind: 'css', selector: '#s' } }, value: 'Saved' };
  const countVisibilityChanges = "()=>{window.visibilityChanges=0;document.addEventListener('visibilitychange',()=>{window.visibilityChanges++})}";
  const visibilityReport = '()=>({state:document.visibilityState,changes:window.visibilityChanges})';
  const click = page => page.click('#b', { timeoutMs: 8000, actionOptions: { expect: saved } });
  const scenarios = {
    click: async ({ a }) => ({ behind: await settle(async () => await click(a), 3000) }),
    hover: async ({ a }) => ({ behind: await settle(async () => await a.hover('#b', { timeoutMs: 8000 }), 3000) }),
    front: async ({ b }) => ({ front: await settle(async () => await click(b), 3000) }),
    async together({ a, b }) {
      const [behind, front] = await Promise.all([settle(async () => await click(a), 5000), settle(async () => await click(b), 5000)]);
      return { behind, front };
    },
    async fill({ a }) {
      const behind = await settle(async () => await a.fill('#n', 'hello', { timeoutMs: 8000 }), 8000);
      const value = await settle(async () => await a.evaluate("()=>document.querySelector('#n').value"), 5000);
      return { behind, value };
    },
  };
  const stages = new Map();
  const results = new Map();
  let fixture;

  async function run(name) {
    const stage = stages.get(name);
    const result = await scenarios[name](stage);
    const visibility = await settle(async () => await Promise.all([stage.a.evaluate(visibilityReport), stage.b.evaluate(visibilityReport)]), 5000);
    return { ...result, visibility };
  }

  function resultOf(name) {
    if (!results.has(name)) {
      const pending = run(name);
      pending.catch(() => {});
      results.set(name, pending);
    }
    return results.get(name);
  }

  function describeAction(outcome) {
    return describeOutcome(outcome.state === 'resolved' && typeof outcome.value === 'object' ? { ...outcome, value: `outcome ${outcome.value?.outcome}` } : outcome);
  }

  function observed(result) {
    return Object.entries(result).map(([name, outcome]) => `${name}: ${name === 'visibility' ? describeOutcome(outcome) : describeAction(outcome)}`).join('; ');
  }

  function untouched(result) {
    const [behind, front] = resolution(result.visibility);
    assert.deepEqual({ behind, front }, { behind: { state: 'hidden', changes: 0 }, front: { state: 'visible', changes: 0 } });
  }

  const rows = [
    {
      title: 'a click on a tab that has been behind for 3 s resolves within 3 s as verified_complete',
      scenario: 'click',
      check: ({ behind }) => assert.equal(resolution(behind).outcome, 'verified_complete'),
    },
    { title: 'that click leaves the tab hidden, the front tab visible and fires no visibilitychange', scenario: 'click', check: untouched },
    { title: 'a hover on a tab that has been behind for 3 s resolves within 3 s', scenario: 'hover', check: ({ behind }) => resolution(behind) },
    { title: 'that hover leaves the tab hidden, the front tab visible and fires no visibilitychange', scenario: 'hover', check: untouched },
    {
      title: 'a click on the front tab still resolves within 3 s',
      scenario: 'front',
      check: ({ front }) => assert.equal(resolution(front).outcome, 'verified_complete'),
    },
    {
      title: 'a click on the tab behind and a click on the front tab issued together both resolve within 5 s',
      scenario: 'together',
      check({ behind, front }) {
        resolution(behind);
        resolution(front);
      },
    },
    { title: 'those two clicks leave the tab behind hidden, the front tab visible and fire no visibilitychange', scenario: 'together', check: untouched },
    {
      title: 'pinned: fill on a tab that has been behind for 3 s resolves and the field holds the text',
      scenario: 'fill',
      check({ behind, value }) {
        resolution(behind);
        assert.equal(resolution(value), 'hello');
      },
    },
    { title: 'that fill leaves the tab hidden, the front tab visible and fires no visibilitychange', scenario: 'fill', check: untouched },
  ];

  before(async () => {
    fixture = await startFixture(pointerDocument);
    const names = Object.keys(scenarios);
    const launched = await Promise.allSettled(names.map(() => launchBrowser()));
    names.forEach((name, index) => {
      if (launched[index].status === 'fulfilled') stages.set(name, { browser: launched[index].value });
    });
    const failed = launched.find(result => result.status === 'rejected');
    if (failed) throw failed.reason;
    await Promise.all(names.map(async name => {
      const stage = stages.get(name);
      stage.a = await stage.browser.newPage(fixture.pageUrl(`${name}-a`));
      stage.b = await stage.browser.newPage(fixture.pageUrl(`${name}-b`));
      await stage.a.execute(countVisibilityChanges);
      await stage.b.execute(countVisibilityChanges);
    }));
    await delay(3000);
  }, { timeout: 120000 });

  after(async () => {
    await closeQuietly(...[...stages.values()].map(stage => stage.browser));
    await fixture?.close();
  }, { timeout: 60000 });

  for (const row of rows) {
    test(`K16: ${row.title}`, { timeout: 60000 }, async t => {
      const result = await resultOf(row.scenario);
      t.diagnostic(observed(result));
      await row.check(result);
    });
  }
});

describe('browsers shutting down together', { skip: skipRealBrowser }, () => {
  test('close race: eight browsers in one process close at once without a rejection, three rounds', { timeout: 300000 }, async t => {
    const rejected = [];
    for (let round = 1; round <= 3; round++) {
      const started = performance.now();
      const launched = await Promise.allSettled(Array.from({ length: 8 }, () => Browser.launch({ ...launchOptions(), timeoutMs: 90000 })));
      const browsers = launched.filter(result => result.status === 'fulfilled').map(result => result.value);
      let closed;
      try {
        const failed = launched.find(result => result.status === 'rejected');
        if (failed) throw failed.reason;
        await Promise.all(browsers.map(async browser => {
          const page = await browser.newPage('data:text/html,<title>t</title>');
          await page.title();
        }));
      } finally {
        closed = await Promise.allSettled(browsers.map(browser => browser.close()));
      }
      const failures = closed.filter(result => result.status === 'rejected').map(result => result.reason);
      const codes = [...new Set(failures.map(error => error?.code))].join(', ');
      t.diagnostic(`round ${round}: ${failures.length} of ${closed.length} close() calls rejected${failures.length ? ` with ${codes}` : ''} in ${Math.round(performance.now() - started)} ms`);
      rejected.push(...failures);
    }
    assert.deepEqual(rejected.map(error => `${error?.code}: ${error?.message}`), []);
  });
});
