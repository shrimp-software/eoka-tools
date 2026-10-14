import { randomUUID } from "node:crypto";
import { Browser, EokaError } from "@cbxss/eoka";

const serverPath = process.argv[2] ?? process.env.EOKA_SERVER_BIN;
const executablePath = process.argv[3] ?? process.env.EOKA_CHROME_PATH;
if (!serverPath || !executablePath) {
  throw new Error("Supply both server and Chrome paths as arguments or EOKA_SERVER_BIN/EOKA_CHROME_PATH");
}
const browser = await Browser.launch({
  serverPath,
  executablePath,
  headless: true,
  sandbox: true,
  timeoutMs: 15000,
});
const operationId = randomUUID();
try {
  const html = '<!doctype html><title>eoka SDK example</title><label>Name <input id="name"></label>';
  const page = await browser.newPage(`data:text/html,${encodeURIComponent(html)}`, { timeoutMs: 15000 });
  const target = { query: { kind: "css", selector: "#name" } };
  const receipt = await page.fill(target, "Ada", {
    operationId,
    timeoutMs: 10000,
    actionOptions: { expect: { kind: "value", target, value: "Ada" }, timeout_ms: 5000 },
  });
  console.log(JSON.stringify({ ownership: browser.ownership, browserVersion: browser.browserVersion, operationId, receipt }, null, 2));
  console.log(JSON.stringify(await page.invoke("page.evaluate", { pageId: page.id, js: '({name: document.querySelector("#name").value})' })));
} catch (error) {
  const primary = error instanceof AggregateError ? error.cause : error;
  if (primary instanceof EokaError && primary.operationId) {
    console.error(JSON.stringify({ code: primary.code, operationId: primary.operationId, requestState: primary.requestState, receipt: primary.receipt }));
    try {
      const record = await browser.operation(primary.operationId).get({ timeoutMs: 5000 });
      console.error(JSON.stringify({ recovered: record }));
    } catch (recoveryError) {
      console.error("Operation recovery failed:", recoveryError);
    }
  }
  throw error;
} finally {
  await browser.close();
}
