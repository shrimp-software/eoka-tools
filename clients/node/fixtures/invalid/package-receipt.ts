import type { Page } from "@cbxss/eoka";

declare const page: Page;
const result: string = await page.fill("#name", "Ada");
export { result };
