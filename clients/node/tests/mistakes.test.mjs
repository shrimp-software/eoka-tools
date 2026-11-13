import assert from 'node:assert/strict';
import { mkdtemp, readFile, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { basename, isAbsolute, join } from 'node:path';
import { after, before, describe, test } from 'node:test';
import { setTimeout as delay } from 'node:timers/promises';
import { fileURLToPath } from 'node:url';
import { ConfigurationError } from '../dist/index.js';
import {
  callerFrames, describeOutcome, launchBrowser, rejection, resolution, settle, skipRealBrowser, startFixture,
} from './fixtures/eval-support.mjs';

const thisFile = basename(fileURLToPath(import.meta.url));
const mangledRedaction = /\w\[redacted\]|\[redacted\]\w/;
const visible = selector => ({ kind: 'visible', target: { query: { kind: 'css', selector } } });
const neverUrl = { kind: 'url', value: '/never', contains: true };
const saved = { kind: 'text', target: { query: { kind: 'css', selector: '#status' } }, value: 'Saved' };

const scenarios = {
  fillMissing: {
    budgetMs: 4000,
    call: async ({ page }) => await page.fill('#missing', 'x', { timeoutMs: 2000 }),
  },
  fillMissingWireTimeout: {
    budgetMs: 4000,
    call: async ({ page }) => await page.fill('#missing', 'x', { actionOptions: { timeout_ms: 2000 } }),
  },
  fillMissingDefault: {
    budgetMs: 32000,
    background: true,
    call: async ({ page }) => await page.fill('#missing', 'x'),
  },
  clickMissing: {
    budgetMs: 4000,
    call: async ({ page }) => await page.click('#missing', { timeoutMs: 2000, actionOptions: { expect: visible('#nope') } }),
  },
  clickDisabled: {
    budgetMs: 4000,
    call: async ({ page }) => await page.click('#off', { timeoutMs: 2000, actionOptions: { expect: visible('#nope') } }),
  },
  fillReadonly: {
    budgetMs: 4000,
    call: async ({ page }) => await page.fill('#ro', 'y', { timeoutMs: 2000 }),
  },
  unmetExpectation: {
    budgetMs: 4000,
    call: async ({ page }) => await page.click('#real', { timeoutMs: 2000, actionOptions: { expect: neverUrl } }),
  },
  strictActionOptions: {
    budgetMs: 4000,
    call: async ({ page }) => await page.click('#real', { actionOptions: { expect: saved, timeoutMs: 2000 } }),
  },
  getTextMissing: {
    budgetMs: 2000,
    call: async ({ page }) => await page.getText('#missing'),
  },
  evaluateFunction: {
    budgetMs: 5000,
    call: async ({ page }) => await page.evaluate(() => document.title),
  },
  evaluateFunctionArguments: {
    budgetMs: 5000,
    call: async ({ page }) => await page.evaluate((a, b) => a + b, [2, 3]),
  },
  evaluateString: {
    budgetMs: 5000,
    call: async ({ page }) => await page.evaluate('()=>document.title'),
  },
  evaluateThrows: {
    budgetMs: 5000,
    call: async ({ page }) => await page.evaluate("()=>{throw new Error('kaboom')}"),
  },
  evaluateOversized: {
    budgetMs: 10000,
    call: async ({ page }) => await page.evaluate("()=>'x'.repeat(2*1024*1024)"),
  },
  screenshotOntoExistingFile: {
    budgetMs: 5000,
    call: async ({ page, existingFile }) => await page.screenshotToFile({ path: existingFile }),
  },
};

function expectActionTimeout(outcome, ...patterns) {
  const error = rejection(outcome);
  const observed = describeOutcome(outcome);
  assert.equal(error.code, 'action_timeout', `expected code action_timeout; ${observed}`);
  expectMessage(outcome, ...patterns);
}

function expectMessage(outcome, ...patterns) {
  const { message } = rejection(outcome);
  for (const pattern of patterns) {
    if (typeof pattern === 'string') assert.ok(message.includes(pattern), `expected the message to include ${JSON.stringify(pattern)}; ${describeOutcome(outcome)}`);
    else assert.match(message, pattern, `expected the message to match ${pattern}; ${describeOutcome(outcome)}`);
  }
}

function expectCallerFrame(outcome) {
  const error = rejection(outcome);
  assert.ok(callerFrames(error).includes(thisFile), `expected a stack frame in ${thisFile}; ${describeOutcome(outcome)}\n${error.stack}`);
}

const rows = [
  {
    id: 'K4',
    title: 'fill on a missing selector rejects as action_timeout naming the selector',
    scenario: 'fillMissing',
    check: outcome => expectActionTimeout(outcome, '#missing', /not found/i),
  },
  {
    id: 'K4',
    title: 'click on a missing selector rejects as action_timeout naming the selector',
    scenario: 'clickMissing',
    check: outcome => expectActionTimeout(outcome, '#missing', /not found/i),
  },
  {
    id: 'K4',
    title: 'fill on a missing selector with no timeout options rejects as action_timeout within 32 s',
    scenario: 'fillMissingDefault',
    check: outcome => expectActionTimeout(outcome),
  },
  {
    id: 'K5',
    title: 'click on a disabled button says the selector is disabled',
    scenario: 'clickDisabled',
    check: outcome => expectMessage(outcome, '#off', /disabled/i),
  },
  {
    id: 'K5',
    title: 'fill on a readonly input says the selector is readonly',
    scenario: 'fillReadonly',
    check: outcome => expectMessage(outcome, '#ro', /readonly|not editable/i),
  },
  {
    id: 'K6',
    title: 'an unmet expectation rejects as action_timeout, names the expectation and says the click was sent',
    scenario: 'unmetExpectation',
    async check(outcome) {
      expectActionTimeout(outcome, '/never', /expect/i, /dispatched|was sent/i);
      for (let waited = 0; outcome.clicks() === 0 && waited < 2000; waited += 50) await delay(50);
      assert.equal(outcome.clicks(), 1, `expected the fixture to see exactly one click; ${describeOutcome(outcome)}`);
    },
  },
  {
    id: 'K7',
    title: 'a camelCase key inside actionOptions is a configuration error before dispatch',
    scenario: 'strictActionOptions',
    check(outcome) {
      const error = rejection(outcome);
      const observed = describeOutcome(outcome);
      assert.ok(error instanceof ConfigurationError, `expected a ConfigurationError; ${observed}`);
      assert.equal(error.code, 'Configuration', `expected code Configuration; ${observed}`);
      assert.equal(error.requestState, 'not_sent', `expected requestState not_sent; ${observed}`);
      expectMessage(outcome, 'timeoutMs', 'timeout_ms');
    },
  },
  {
    id: 'K7',
    title: 'a camelCase key inside actionOptions never clicks the page',
    scenario: 'strictActionOptions',
    async check(outcome) {
      await delay(500);
      assert.equal(outcome.clicks(), 0, `expected the fixture to see no click; ${describeOutcome(outcome)}`);
    },
  },
  {
    id: 'K8',
    title: 'evaluate accepts a function',
    scenario: 'evaluateFunction',
    check: outcome => assert.equal(resolution(outcome), 'fixture'),
  },
  {
    id: 'K8',
    title: 'evaluate passes arguments to a function',
    scenario: 'evaluateFunctionArguments',
    check: outcome => assert.equal(resolution(outcome), 5),
  },
  {
    id: 'K8',
    title: 'evaluate still accepts the string form',
    scenario: 'evaluateString',
    check: outcome => assert.equal(resolution(outcome), 'fixture'),
  },
  {
    id: 'K9',
    title: 'a rejected read carries the caller frame',
    scenario: 'getTextMissing',
    check: expectCallerFrame,
  },
  {
    id: 'K9',
    title: 'a rejected action carries the caller frame',
    scenario: 'fillMissing',
    check: expectCallerFrame,
  },
  {
    id: 'K9',
    title: 'a configuration error carries the caller frame',
    scenario: 'strictActionOptions',
    check(outcome) {
      const error = rejection(outcome);
      assert.ok(error instanceof ConfigurationError, `expected a ConfigurationError; ${describeOutcome(outcome)}`);
      expectCallerFrame(outcome);
    },
  },
  {
    id: 'K11',
    title: 'redaction of the typed value leaves the timeout message intact',
    scenario: 'fillMissing',
    check(outcome) {
      expectMessage(outcome, 'exceeded');
      assert.doesNotMatch(rejection(outcome).message, mangledRedaction, describeOutcome(outcome));
    },
  },
  {
    id: 'K11',
    title: 'redaction leaves the timeout message intact when the deadline is given as actionOptions.timeout_ms',
    scenario: 'fillMissingWireTimeout',
    check(outcome) {
      expectMessage(outcome, 'exceeded');
      assert.doesNotMatch(rejection(outcome).message, mangledRedaction, describeOutcome(outcome));
    },
  },
  {
    id: 'K12',
    title: 'screenshotToFile onto an existing file names the absolute path and says it exists',
    scenario: 'screenshotOntoExistingFile',
    async check(outcome, { existingFile }) {
      assert.ok(isAbsolute(existingFile));
      expectMessage(outcome, existingFile, /exists/i);
      assert.equal(await readFile(existingFile, 'utf8'), 'occupied');
    },
  },
  {
    id: 'pinned',
    title: 'getText on a missing selector rejects fast as ElementNotFound naming the selector',
    scenario: 'getTextMissing',
    check(outcome) {
      const error = rejection(outcome);
      assert.equal(error.code, 'ElementNotFound', describeOutcome(outcome));
      expectMessage(outcome, '#missing');
    },
  },
  {
    id: 'pinned',
    title: 'evaluate of a throwing function surfaces the thrown message',
    scenario: 'evaluateThrows',
    check: outcome => expectMessage(outcome, 'kaboom'),
  },
  {
    id: 'pinned',
    title: 'an evaluate result over 1 MiB rejects as MessageTooLarge',
    scenario: 'evaluateOversized',
    check: outcome => assert.equal(rejection(outcome).code, 'MessageTooLarge', describeOutcome(outcome)),
  },
];

describe('caller mistakes', { skip: skipRealBrowser }, () => {
  let fixture;
  let browser;
  let backgroundBrowser;
  let directory;
  let existingFile;
  const outcomes = new Map();

  async function run(name) {
    const scenario = scenarios[name];
    const page = await (scenario.background ? backgroundBrowser : browser).newPage(fixture.pageUrl(name));
    const outcome = await settle(() => scenario.call({ page, existingFile }), scenario.budgetMs);
    return { ...outcome, clicks: () => fixture.clicks(name) };
  }

  function outcomeOf(name) {
    if (!outcomes.has(name)) {
      const pending = run(name);
      pending.catch(() => {});
      outcomes.set(name, pending);
    }
    return outcomes.get(name);
  }

  before(async () => {
    fixture = await startFixture();
    directory = await mkdtemp(join(tmpdir(), 'eoka-mistakes-'));
    existingFile = join(directory, 'existing.png');
    await writeFile(existingFile, 'occupied');
    [browser, backgroundBrowser] = await Promise.all([launchBrowser(), launchBrowser()]);
    for (const [name, scenario] of Object.entries(scenarios)) {
      if (scenario.background) outcomeOf(name);
    }
  }, { timeout: 90000 });

  after(async () => {
    try {
      await Promise.all([browser, backgroundBrowser].filter(Boolean).map(instance => instance.close()));
    } finally {
      await fixture?.close();
      if (directory) await rm(directory, { recursive: true, force: true });
    }
  }, { timeout: 60000 });

  const foreground = rows.filter(row => !scenarios[row.scenario].background);
  const background = rows.filter(row => scenarios[row.scenario].background);
  for (const row of [...foreground, ...background]) {
    test(`${row.id}: ${row.title}`, { timeout: scenarios[row.scenario].budgetMs + 30000 }, async t => {
      const outcome = await outcomeOf(row.scenario);
      t.diagnostic(`${row.scenario}: ${describeOutcome(outcome)}; fixture clicks ${outcome.clicks()}`);
      await row.check(outcome, { existingFile });
    });
  }
});
