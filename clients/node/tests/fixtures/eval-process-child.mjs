import { once } from 'node:events';
import { Browser } from '../../dist/index.js';

const [mode, url] = process.argv.slice(2);
const browser = await Browser.launch({
  serverPath: process.env.EOKA_RELIABILITY_SERVER,
  executablePath: process.env.EOKA_RELIABILITY_CHROME,
  headless: true,
  timeoutMs: 30000,
});
const page = await browser.newPage(url);
process.stdin.once('end', () => process.exit(0));
process.stdout.write(`ready ${await page.title()}\n`);
await once(process.stdin, 'data');
if (mode === 'throw') throw new Error('uncaught fixture failure');
await new Promise(() => {});
