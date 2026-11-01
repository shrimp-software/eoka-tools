import assert from 'node:assert/strict';
import { test } from 'node:test';
import { createServer } from 'node:http';
import { once } from 'node:events';
import { Browser } from '../dist/index.js';

test('real browser preserves sequential input, scoped references, exceptions and popup destinations', {
  skip: !process.env.EOKA_RELIABILITY_SERVER,
  timeout: 90000,
}, async () => {
  assert.ok(process.env.EOKA_RELIABILITY_CHROME);
  const server = createServer((request, response) => {
    response.setHeader('Content-Type', 'text/html');
    response.end(request.url === '/child'
      ? '<button id=ready disabled>Ready</button><script>setTimeout(()=>ready.disabled=false,250)</script>'
      : '<label>Name<input id=name oninput="events++" onblur="blurred=true"></label><a href=/child target=_blank>Continue</a><script>window.events=0;window.blurred=false</script>');
  });
  server.listen(0, '127.0.0.1');
  await once(server, 'listening');
  let browser;
  try {
    browser = await Browser.launch({ serverPath: process.env.EOKA_RELIABILITY_SERVER, executablePath: process.env.EOKA_RELIABILITY_CHROME, headless: true, timeoutMs: 30000 });
    const url = `http://127.0.0.1:${server.address().port}`;
    const page = await browser.newPage(url);
    const first = await page.observe();
    const field = first.controls.find(control => control.name === 'Name').target;
    const link = first.controls.find(control => control.name === 'Continue').target;
    const filled = await page.step({ kind: 'fill', target: field, text: 'aé😀' });
    assert.equal(filled.status, 'verified_complete');
    assert.equal(await page.evaluate('()=>window.events'), 3);
    await page.pressKey('Tab', { actionOptions: { expect: { kind: 'focused', target: { query: { kind: 'css', selector: 'a' } } } } });
    assert.equal(await page.evaluate('()=>window.blurred'), true);
    for (let count = 0; count < 2; count++) {
      assert.equal(await page.evaluate('value=>{const local=value;return Object.hasOwn(local,"__proto__")}', [JSON.parse('{"__proto__":42}')]), true);
    }
    await assert.rejects(page.execute('async()=>{throw new Error("fixture-exception")}'), /fixture-exception/);
    for (const fn of ['()=>({missing:undefined})', '()=>({n:NaN})', '()=>9007199254740992', '()=>new Date(0)']) {
      await assert.rejects(page.evaluate(fn), /JSON|number/);
    }
    assert.equal((await page.evaluate("()=> 'é'.repeat(524287)")).length, 524287);
    await assert.rejects(page.evaluate("()=> 'é'.repeat(524288)"), error => error.code === 'MessageTooLarge');
    await page.execute('()=>{const v={};v.self=v;return v}');
    const rejected = await page.step({ kind: 'click', target: link });
    assert.equal(rejected.status, 'not_dispatched');
    const clicked = await page.step({ kind: 'click', target: link }, { expect: { kind: 'all', conditions: [
      { kind: 'url', value: `${url}/child` },
      { kind: 'enabled', target: { query: { kind: 'css', selector: '#ready' } }, enabled: true },
    ] }, destination: 'new_tab' });
    assert.equal(clicked.status, 'verified_complete');
    assert.notEqual(clicked.observation.tab_id, page.id);
    assert.equal(await browser.page(clicked.observation.tab_id).evaluate('()=>document.querySelector("#ready").disabled'), false);
  } finally {
    if (browser) await browser.close();
    server.closeAllConnections();
    await new Promise(resolve => server.close(resolve));
  }
});
