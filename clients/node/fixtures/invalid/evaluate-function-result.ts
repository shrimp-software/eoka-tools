import type { Page } from "@cbxss/eoka";

declare const page: Page;
const title: number = await page.evaluate(() => "title");
export { title };
