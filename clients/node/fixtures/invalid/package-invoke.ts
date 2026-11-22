import type { Page } from "@cbxss/eoka";

declare const page: Page;
await page.invoke("eval", { pageId: page.id, code: 42 });
