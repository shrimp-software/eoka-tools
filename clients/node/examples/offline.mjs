import { Browser } from "@cbxss/eoka";

const browser = await Browser.launch({
  ...(process.argv[2] === undefined ? {} : { browserCacheDir: process.argv[2] }),
  download: "never",
  headless: true,
  sandbox: true,
  timeoutMs: 30000,
});
try {
  const page = await browser.newPage("data:text/html,%3Ctitle%3Eeoka%20offline%3C/title%3E");
  const result = await page.invoke("page.evaluate", { pageId: page.id, js: "document.title" });
  console.log(JSON.stringify({
    title: result.result,
    ownership: browser.ownership,
    browserVersion: browser.browserVersion,
  }, null, 2));
} finally {
  await browser.close();
}
