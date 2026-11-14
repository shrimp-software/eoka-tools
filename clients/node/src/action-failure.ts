import type { Action, Expectation, OperationRecord } from './generated/protocol.js';
import { blockedPage, inspectBeforeContinuing, nodeNextStep } from './page-blockers.js';

type Fields = Record<string, unknown>;

const preparationReasons = ['target not found', 'target disconnected', 'target disabled', 'target hidden', 'target readonly or not editable', 'target moving', 'target covered'];
const notRolledBack = `The action was not rolled back; ${inspectBeforeContinuing}.`;
const interruptions = {
  dialog: ['then a dialog blocked the page before the result could be verified', 'could not start because a dialog is open on the page'],
  navigation: ['then a pending navigation blocked the page before the result could be verified', 'could not start because the page is waiting on a navigation'],
  unresponsive: ['then the page stopped answering before the result could be verified', 'could not start because the page is not answering'],
} as const;

function fields(value: unknown): Fields | undefined {
  return value !== null && typeof value === 'object' && !Array.isArray(value) ? value as Fields : undefined;
}

function locatorLabel(target: unknown): string {
  const query = fields(fields(target)?.query);
  if (!query) return 'the target';
  if (query.kind === 'css') return `"${String(query.selector)}"`;
  if (query.kind === 'role') return `role "${String(query.role)}"`;
  if (query.kind === 'text' || query.kind === 'placeholder') return `${query.kind} "${String(query.text)}"`;
  return `${String(query.kind)} ${String(query.id)}`;
}

function addressLabel(address: string): string {
  try {
    const url = new URL(address);
    return `"${`${url.origin === 'null' ? url.protocol : url.origin}${url.pathname}`.slice(0, 200)}"`;
  } catch {
    return 'the requested URL';
  }
}

function subject(action: Fields): string {
  const kind = String(action.kind);
  if (action.target !== undefined) return `${kind} ${locatorLabel(action.target)}`;
  if (typeof action.key === 'string') return `${kind} "${action.key}"`;
  if (typeof action.url === 'string') return `${kind} ${addressLabel(action.url)}`;
  return kind;
}

function expectationLabel(expectation: unknown): string {
  const expect = fields(expectation);
  if (!expect) return 'the expectation';
  const target = locatorLabel(expect.target);
  switch (expect.kind) {
    case 'url': return `url ${expect.contains === true ? 'contains' : 'equals'} "${String(expect.value)}"`;
    case 'text': return `text of ${target} ${expect.contains === true ? 'contains' : 'equals'} "${String(expect.value)}"`;
    case 'visible':
    case 'hidden':
    case 'focused': return `${target} is ${expect.kind}`;
    case 'enabled': return `${target} is ${expect.enabled === false ? 'disabled' : 'enabled'}`;
    case 'checked': return `${target} is ${expect.checked === false ? 'unchecked' : 'checked'}`;
    case 'value': return `${target} has the expected value`;
    case 'selected': return `${target} has the expected selection`;
    case 'all': return Array.isArray(expect.conditions) ? expect.conditions.map(expectationLabel).join(' and ') : 'all conditions';
    case 'document_ready': return 'the document is ready';
    case 'download_started': return 'a download starts';
    default: return String(expect.kind);
  }
}

function submittedValues(action: Fields): string[] {
  return [action.value, action.text].filter((value): value is string => typeof value === 'string' && value !== '');
}

export function actionFailureMessage(action: Action, expect: Expectation | null | undefined, timeoutMs: number | undefined, record: OperationRecord): string {
  const detail = record.error;
  if (record.state === 'running' || detail == null) {
    return `Operation ${record.id} is ${record.state} with outcome ${record.outcome}; verified completion was not established.`;
  }
  const what = subject(action);
  const dispatch = record.receipt?.dispatch ?? (record.dispatch_intent ? 'may_have_been_sent' : 'not_sent');
  const dispatched = dispatch === 'acknowledged' ? 'was dispatched' : 'may have been dispatched';
  const blocked = blockedPage(record.error_code, detail.code);
  if (blocked !== undefined) {
    const told = detail.message.trimEnd();
    const report = `${told}${/[.!?]$/.test(told) ? '' : '.'} ${nodeNextStep(blocked)}`;
    if (blocked === 'navigation_timeout') {
      return dispatch === 'not_sent' ? `${what} did not complete. ${report}` : `${what} ${dispatched}, but the navigation got no response. ${report}`;
    }
    const [interrupted, refused] = interruptions[blocked];
    return dispatch === 'not_sent' ? `${what} ${refused}. ${report} The page received no input.` : `${what} ${dispatched}, ${interrupted}. ${report} ${notRolledBack}`;
  }
  if (record.error_code !== 'action_timeout') {
    const reason = detail.message.replace(/[.\s]+$/, '');
    return dispatch === 'not_sent' ? `${what} failed: ${reason}. The page received no input.` : `${what} ${dispatched} and failed: ${reason}. ${notRolledBack}`;
  }
  const limit = timeoutMs === undefined ? 'its deadline' : `${timeoutMs} ms`;
  if (dispatch !== 'not_sent') {
    const unmet = expect == null ? 'its result was not verified' : `its expectation (${expectationLabel(expect)}) was not met`;
    return `${what} ${dispatched}, but ${unmet} within ${limit}. ${notRolledBack}`;
  }
  const texts = [detail.message, ...submittedValues(action).map(value => detail.message.replaceAll('[redacted]', value))];
  const mentions = (text: string): boolean => texts.some(candidate => candidate.includes(text));
  const reason = preparationReasons.find(mentions);
  const waiting = reason !== undefined ? `: ${reason}` : mentions('queue admission') ? ' while waiting for earlier actions on this page' : '';
  return `${what} timed out after ${limit}${waiting}. The page received no input.`;
}
