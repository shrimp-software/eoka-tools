import type { PageBlocker } from './generated/protocol.js';

type Fields = Record<string, unknown>;
export type BlockedPage = 'dialog' | 'navigation' | 'unresponsive' | 'navigation_timeout';

const blockedPages: Record<string, BlockedPage> = {
  DialogOpen: 'dialog',
  dialog_open: 'dialog',
  NavigationPending: 'navigation',
  navigation_pending: 'navigation',
  PageUnresponsive: 'unresponsive',
  page_unresponsive: 'unresponsive',
  navigation_timeout: 'navigation_timeout',
};

export const inspectBeforeContinuing = 'inspect the page before deciding what to do next';

function fields(value: unknown): Fields | undefined {
  return value !== null && typeof value === 'object' && !Array.isArray(value) ? value as Fields : undefined;
}

export function blockedPage(...codes: unknown[]): BlockedPage | undefined {
  for (const code of codes) {
    if (typeof code === 'string' && Object.hasOwn(blockedPages, code)) return blockedPages[code];
  }
  return undefined;
}

export function pageBlocker(data: unknown): PageBlocker | undefined {
  const blocker = fields(fields(data)?.blocker);
  if (blocker?.kind === 'dialog' && typeof blocker.message === 'string') return blocker as PageBlocker;
  if (blocker?.kind === 'navigation') return blocker as PageBlocker;
  return undefined;
}

export function nodeNextStep(blocked: BlockedPage, blocker?: PageBlocker): string {
  if (blocked === 'navigation') {
    return 'Keep waiting, for example with page.ops.wait({ url, timeout }), or abandon the load: page.goto(url) navigates again and page.ops.reload() returns to the page that is still displayed.';
  }
  if (blocked === 'unresponsive') {
    return 'Give the page time to settle; if it stays frozen, continue in a new tab with browser.newPage(url). After Browser.connect, page.ops.dialogDismiss() clears a dialog that was already open when the browser was attached.';
  }
  if (blocked === 'navigation_timeout') {
    return 'Allow more time if the site is slow (timeoutMs on page.goto), or check the URL, network and proxy.';
  }
  const dialogType = blocker?.kind === 'dialog' ? blocker.dialogType : undefined;
  if (dialogType === 'beforeunload') return 'Call page.ops.dialogAccept() to leave the page or page.ops.dialogDismiss() to stay.';
  if (dialogType === 'prompt') return 'Call page.ops.dialogAccept({ prompt_text }) to answer the prompt or page.ops.dialogDismiss() to cancel it.';
  return 'Call page.ops.dialogAccept() or page.ops.dialogDismiss() on this page to close the dialog.';
}

export function withNodeNextStep(message: string, codes: readonly unknown[], data: unknown): string {
  const blocked = blockedPage(...codes);
  if (blocked === undefined) return message;
  const sentence = message.trimEnd();
  const interrupted = fields(fields(fields(data)?.detail)?.context)?.action_status === 'may_have_executed';
  const consequence = interrupted ? ` The interrupted call may already have changed the page; ${inspectBeforeContinuing}.` : '';
  return `${sentence}${/[.!?]$/.test(sentence) ? '' : '.'} ${nodeNextStep(blocked, pageBlocker(data))}${consequence}`;
}
