import type { Page } from "@cbxss/eoka";

declare const page: Page;
await page.invoke("page.evaluate", { pageId: page.id, js: 42 });
