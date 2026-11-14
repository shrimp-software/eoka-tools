import type { EokaError } from "@cbxss/eoka";

declare const error: EokaError;
const text: string = error.blocker.message;
export { text };
