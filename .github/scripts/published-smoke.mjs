import assert from 'node:assert/strict';
import { once } from 'node:events';
import { createServer } from 'node:http';
import { Browser, releaseManifest } from '@cbxss/eoka';

const expectedNode = process.env.EXPECTED_NODE_VERSION;
if (expectedNode) assert.equal(process.versions.node, expectedNode, 'the smoke is not running on the Node version under test');

const server = createServer((request, response) => {
  response.writeHead(200, { 'content-type': 'text/html' }).end('<!doctype html><title>published smoke</title><h1>ok</h1>');
});
server.listen(0, '127.0.0.1');
await once(server, 'listening');
const started = performance.now();
let browser;
try {
  browser = await Browser.launch({ timeoutMs: 600000 });
  const page = await browser.newPage(`http://127.0.0.1:${server.address().port}/`);
  assert.equal(await page.title(), 'published smoke');
  console.log(JSON.stringify({
    status: 'passed',
    node: process.versions.node,
    sdkVersion: releaseManifest.sdkVersion,
    browserVersion: browser.browserVersion,
    elapsedMs: Math.round(performance.now() - started),
  }));
} finally {
  await browser?.close();
  server.closeAllConnections();
  await new Promise(resolve => server.close(resolve));
}
