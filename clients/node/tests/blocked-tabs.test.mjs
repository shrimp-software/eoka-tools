import assert from 'node:assert/strict';
import { after, before, describe, test } from 'node:test';
import { setTimeout as delay } from 'node:timers/promises';
import { EokaError } from '../dist/index.js';
import {
  closeQuietly, describeOutcome, launchBrowser, rejection, resolution, settle, skipRealBrowser, startFixture,
} from './fixtures/eval-support.mjs';

const fixtureDocument = `<!doctype html><title>fixture</title>
<h1>Blocked tabs</h1>
<button id="danger" onclick="report();alert('are you sure?');document.getElementById('s').textContent='Deleted'">Delete</button>
<p id="s"></p>
<script>function report(){const request=new XMLHttpRequest();request.open('POST','/hit'+location.search,false);request.send()}</script>`;
const openAlertFromTimer = "()=>{setTimeout(()=>alert('are you sure?'),0)}";
const navigateToHangFromTimer = "()=>{setTimeout(()=>{location.href='/hang'},0)}";
const deleted = { kind: 'text', target: { query: { kind: 'css', selector: '#s' } }, value: 'Deleted' };
const hint = /dialogAccept|dialogDismiss/;

async function tabWithDialog(browser, url) {
  const page = await browser.newPage(url);
  await page.execute(openAlertFromTimer);
  await delay(300);
  return page;
}

async function dismissQuietly(page) {
  await settle(async () => await page.ops.dialogDismiss({}, { timeoutMs: 1000 }), 1500);
}

function readBehindDialog(read) {
  return {
    browser: 'reads',
    async run({ browser, url }) {
      const page = await tabWithDialog(browser, url);
      const blocked = await settle(async () => await read(page), 2000);
      await dismissQuietly(page);
      return { blocked };
    },
  };
}

function clearing(clear) {
  return {
    browser: 'clearing',
    async run({ browser, url }) {
      const page = await tabWithDialog(browser, url);
      const cleared = await settle(async () => await clear(page), 2000);
      const title = await settle(async () => await page.title(), 2000);
      if (title.state !== 'resolved') await dismissQuietly(page);
      return { cleared, title };
    },
  };
}

const scenarios = {
  dialogTitle: readBehindDialog(page => page.title()),
  dialogEvaluate: readBehindDialog(page => page.evaluate('()=>1')),
  dialogGetText: readBehindDialog(page => page.getText('h1')),
  dialogExists: readBehindDialog(page => page.exists('h1')),
  confirmFromEvaluate: {
    browser: 'reads',
    async run({ browser, url }) {
      const page = await browser.newPage(url);
      const blocked = await settle(async () => await page.evaluate("()=>confirm('sure?')"), 2000);
      await dismissQuietly(page);
      return { blocked };
    },
  },
  acceptClears: clearing(page => page.ops.dialogAccept()),
  dismissClears: clearing(page => page.ops.dialogDismiss()),
  otherTab: {
    browser: 'clearing',
    async run({ browser, url }) {
      const other = await browser.newPage(url);
      await other.title();
      const blockedTab = await tabWithDialog(browser, url);
      const title = await settle(async () => await other.title(), 1000);
      await dismissQuietly(blockedTab);
      return { title };
    },
  },
  clickOpensDialog: {
    browser: 'clearing',
    async run({ browser, url }) {
      const page = await browser.newPage(url);
      const click = await settle(async () => await page.click('#danger', { timeoutMs: 8000, actionOptions: { expect: deleted } }), 3000);
      const accepted = await settle(async () => await page.ops.dialogAccept(), 5000);
      const text = await settle(async () => await page.getText('#s'), 5000);
      return { click, accepted, text };
    },
  },
  stalledNavigation: {
    browser: 'navigation',
    background: true,
    async run({ browser, url, fixture }) {
      const other = await browser.newPage(url);
      await other.title();
      const page = await browser.newPage(url);
      await page.execute(navigateToHangFromTimer);
      await delay(6500);
      const blocked = await settle(async () => await page.title(), 2000);
      const otherTitle = await settle(async () => await other.title(), 1000);
      const recovered = await settle(async () => await page.goto(`${fixture.origin}/`, { timeoutMs: 8000 }), 8000);
      const title = await settle(async () => await page.title(), 2000);
      return { blocked, otherTitle, recovered, title };
    },
  },
  earlyReadOnStalledNavigation: {
    browser: 'navigation',
    background: true,
    async run({ browser, url }) {
      const page = await browser.newPage(url);
      await page.execute(navigateToHangFromTimer);
      await delay(500);
      const blocked = await settle(async () => await page.title(), 8000);
      return { blocked };
    },
  },
};

function observed(result) {
  const parts = Object.entries(result)
    .filter(([, outcome]) => typeof outcome?.state === 'string')
    .map(([name, outcome]) => `${name}: ${describeOutcome(outcome.state === 'resolved' && typeof outcome.value === 'object' ? { ...outcome, value: 'a result' } : outcome)}`);
  return `${parts.join('; ')}; fixture clicks ${result.clicks()}`;
}

function expectCode(outcome, code) {
  const error = rejection(outcome);
  assert.ok(error instanceof EokaError, `expected an EokaError; ${describeOutcome(outcome)}`);
  assert.equal(error.code, code, `expected code ${code}; ${describeOutcome(outcome)}`);
  return error;
}

function expectDialogOpen({ blocked }) {
  const { message } = expectCode(blocked, 'DialogOpen');
  assert.match(message, /dialog/i, describeOutcome(blocked));
  assert.ok(message.includes('are you sure?'), `expected the message to quote the dialog text; ${describeOutcome(blocked)}`);
}

function expectHint(outcome) {
  assert.match(rejection(outcome).message, hint, `expected the message to name dialogAccept or dialogDismiss; ${describeOutcome(outcome)}`);
}

function expectCleared({ cleared, title }) {
  resolution(cleared);
  assert.equal(resolution(title), 'fixture');
}

const reads = [
  ['title()', 'dialogTitle'],
  ["evaluate('()=>1')", 'dialogEvaluate'],
  ["getText('h1')", 'dialogGetText'],
  ["exists('h1')", 'dialogExists'],
];

const rows = [
  ...reads.flatMap(([call, scenario]) => [
    { id: 'E1', title: `${call} on a tab with an open dialog rejects within 2 s as DialogOpen quoting the dialog`, scenario, check: expectDialogOpen },
    { id: 'E1', title: `the DialogOpen error from ${call} names dialogAccept or dialogDismiss`, scenario, check: ({ blocked }) => expectHint(blocked) },
  ]),
  { id: 'E2', title: 'dialogAccept() resolves within 2 s while the dialog is open and title() answers within 2 s afterwards', scenario: 'acceptClears', check: expectCleared },
  { id: 'E2', title: 'dialogDismiss() resolves within 2 s while the dialog is open and title() answers within 2 s afterwards', scenario: 'dismissClears', check: expectCleared },
  { id: 'E3', title: 'while one tab has a dialog open another tab answers title() in under 1000 ms', scenario: 'otherTab', check: ({ title }) => assert.equal(resolution(title), 'fixture') },
  {
    id: 'E4',
    title: 'a click that opens a dialog rejects within 3 s as dialog_open and says the click was sent',
    scenario: 'clickOpensDialog',
    check({ click }) {
      const { message } = expectCode(click, 'dialog_open');
      assert.match(message, /dialog/i, describeOutcome(click));
      assert.match(message, /dispatched|was sent/i, describeOutcome(click));
    },
  },
  { id: 'E4', title: 'the dialog_open error from the click names dialogAccept or dialogDismiss', scenario: 'clickOpensDialog', check: ({ click }) => expectHint(click) },
  {
    id: 'E4',
    title: 'the click that opened the dialog reached the page exactly once',
    scenario: 'clickOpensDialog',
    check: result => assert.equal(result.clicks(), 1, observed(result)),
  },
  {
    id: 'E4',
    title: "after dialogAccept() the click handler finishes and getText('#s') is Deleted",
    scenario: 'clickOpensDialog',
    check({ accepted, text }) {
      resolution(accepted);
      assert.equal(resolution(text), 'Deleted');
    },
  },
  { id: 'E5', title: 'an evaluate that opens a confirm rejects within 2 s as DialogOpen', scenario: 'confirmFromEvaluate', check: ({ blocked }) => expectCode(blocked, 'DialogOpen') },
  {
    id: 'E6',
    title: 'title() on a tab whose own navigation has stalled for 6.5 s rejects within 2 s as NavigationPending naming /hang',
    scenario: 'stalledNavigation',
    check({ blocked }) {
      const { message } = expectCode(blocked, 'NavigationPending');
      assert.ok(message.includes('/hang'), `expected the message to include /hang; ${describeOutcome(blocked)}`);
    },
  },
  { id: 'E6', title: 'while that navigation is stalled another tab answers title() in under 1000 ms', scenario: 'stalledNavigation', check: ({ otherTitle }) => assert.equal(resolution(otherTitle), 'fixture') },
  {
    id: 'E6',
    title: 'goto() to a working page then resolves within 8 s and title() answers within 2 s',
    scenario: 'stalledNavigation',
    check({ recovered, title }) {
      resolution(recovered);
      assert.equal(resolution(title), 'fixture');
    },
  },
  {
    id: 'E7',
    title: 'title() sent 500 ms after a navigation that then stalls rejects within 8 s as NavigationPending',
    scenario: 'earlyReadOnStalledNavigation',
    check: ({ blocked }) => expectCode(blocked, 'NavigationPending'),
  },
];

describe('blocked tabs', { skip: skipRealBrowser }, () => {
  const groups = [...new Set(Object.values(scenarios).map(scenario => scenario.browser))];
  const browsers = new Map();
  const results = new Map();
  let fixture;

  async function run(name) {
    const scenario = scenarios[name];
    const result = await scenario.run({ browser: browsers.get(scenario.browser), url: fixture.pageUrl(name), fixture });
    return { ...result, clicks: () => fixture.clicks(name) };
  }

  function resultOf(name) {
    if (!results.has(name)) {
      const pending = run(name);
      pending.catch(() => {});
      results.set(name, pending);
    }
    return results.get(name);
  }

  before(async () => {
    fixture = await startFixture(fixtureDocument);
    const launched = await Promise.allSettled(groups.map(() => launchBrowser()));
    groups.forEach((group, index) => {
      if (launched[index].status === 'fulfilled') browsers.set(group, launched[index].value);
    });
    const failed = launched.find(result => result.status === 'rejected');
    if (failed) throw failed.reason;
    for (const [name, scenario] of Object.entries(scenarios)) {
      if (scenario.background) resultOf(name);
    }
  }, { timeout: 120000 });

  after(async () => {
    await closeQuietly(...browsers.values());
    await fixture?.close();
  }, { timeout: 60000 });

  const foreground = rows.filter(row => !scenarios[row.scenario].background);
  const background = rows.filter(row => scenarios[row.scenario].background);
  for (const row of [...foreground, ...background]) {
    test(`${row.id}: ${row.title}`, { timeout: 90000 }, async t => {
      const result = await resultOf(row.scenario);
      t.diagnostic(`${row.scenario}: ${observed(result)}`);
      await row.check(result);
    });
  }
});
