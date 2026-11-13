import type { Page } from "@cbxss/eoka";

declare const page: Page;
await page.fill("#name", "Ada", { actionOptions: { timeoutMs: 2000 } });
