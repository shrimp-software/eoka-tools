import { readFile } from 'node:fs/promises';
import { pathToFileURL } from 'node:url';

const configuration = JSON.parse(await readFile(process.argv[2], 'utf8'));
const { Browser } = await import(pathToFileURL(configuration.browserModule).href);
const controller = new AbortController();
const progress = [];
const start = performance.now();
let browser;
let result;
try {
  const options = {
    ...configuration.options,
    signal: controller.signal,
    ...(configuration.connect ? {} : {
      onProgress: event => {
        progress.push(event);
        if (configuration.abortOnProgress) controller.abort(new Error('abort during installation'));
      },
    }),
  };
  browser = configuration.connect ? await Browser.connect(options) : await Browser.launch(options);
  result = { ok: true, ownership: browser.ownership, browserVersion: browser.browserVersion, serverVersion: browser.hello.serverVersion };
} catch (error) {
  result = { ok: false, code: error.code, message: error.message, errors: error.errors?.map(item => ({ code: item.code, message: item.message })) };
} finally {
  if (browser) await browser.close();
}
process.stdout.write(JSON.stringify({ ...result, elapsedMs: performance.now() - start, progress }));
