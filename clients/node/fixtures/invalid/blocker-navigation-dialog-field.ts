import type { PageBlocker } from "@cbxss/eoka";

declare const blocker: PageBlocker;
const dialogType: string = blocker.kind === "navigation" ? blocker.dialogType : "";
export { dialogType };
