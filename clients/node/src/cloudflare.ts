import type { CallOptions } from './client.js';
import { ConfigurationError } from './errors.js';
import type { Page } from './page.js';

/** Options for the browser-wide Cloudflare challenge watcher. */
export interface CloudflareOptions {
  /** Poll interval in milliseconds. Defaults to 500. */
  intervalMs?: number;
  /** Deadline for each `solveTurnstile()` attempt. Defaults to 30000. */
  timeoutMs?: number;
  /** Called when a full-page challenge is detected, before the click. */
  onChallenge?: (page: Page) => void;
  /** Called after the challenge page has been left. */
  onSolved?: (page: Page) => void;
  /** Called for watcher or solve failures. A page is omitted when listing tabs fails. */
  onError?: (error: unknown, page?: Page) => void;
}

export interface ResolvedCloudflareOptions {
  intervalMs: number;
  timeoutMs: number;
  onChallenge?: (page: Page) => void;
  onSolved?: (page: Page) => void;
  onError?: (error: unknown, page?: Page) => void;
}

export interface CloudflareBrowser {
  tabs(options?: CallOptions): Promise<readonly { id: string }[]>;
  page(id: string): Page;
}

const challengeTitle = /just a moment|attention required|checking your browser|verify you are human|please wait.*cloudflare/i;

/**
 * Full-page Cloudflare challenge titles. A Turnstile widget embedded in a
 * normal form has the same frame URL, so the title is the discriminator.
 */
export function isCloudflareChallengeTitle(title: string): boolean {
  return challengeTitle.test(title);
}

export function resolveCloudflareOptions(value: boolean | CloudflareOptions | undefined): ResolvedCloudflareOptions | undefined {
  if (value === undefined || value === false) return undefined;
  const options = value === true ? {} : value;
  const intervalMs = options.intervalMs ?? 500;
  const timeoutMs = options.timeoutMs ?? 30_000;
  if (!Number.isFinite(intervalMs) || intervalMs < 100 || intervalMs > 60_000) {
    throw new ConfigurationError('autoSolveCloudflare.intervalMs must be between 100 and 60000 milliseconds');
  }
  if (!Number.isFinite(timeoutMs) || timeoutMs <= 0 || timeoutMs > 2_147_483_647) {
    throw new ConfigurationError('autoSolveCloudflare.timeoutMs must be finite, positive and at most 2147483647 milliseconds');
  }
  for (const name of ['onChallenge', 'onSolved', 'onError'] as const) {
    if (options[name] !== undefined && typeof options[name] !== 'function') {
      throw new ConfigurationError(`autoSolveCloudflare.${name} must be a function`);
    }
  }
  const resolved: ResolvedCloudflareOptions = { intervalMs, timeoutMs };
  if (options.onChallenge !== undefined) resolved.onChallenge = options.onChallenge;
  if (options.onSolved !== undefined) resolved.onSolved = options.onSolved;
  if (options.onError !== undefined) resolved.onError = options.onError;
  return resolved;
}

/**
 * Polls every tab for a full-page Cloudflare challenge and clicks its
 * checkbox through `Page.solveTurnstile()`. The timer is unref'd, so it does
 * not keep the Node process alive. `stop()` aborts an in-flight attempt.
 */
export class CloudflareWatcher {
  readonly #browser: CloudflareBrowser;
  readonly #options: ResolvedCloudflareOptions;
  readonly #controller = new AbortController();
  readonly #pages = new Map<string, Page>();
  readonly #solving = new Set<string>();
  #timer: NodeJS.Timeout | undefined;
  #running = false;
  #stopped = false;

  constructor(browser: CloudflareBrowser, options: ResolvedCloudflareOptions) {
    this.#browser = browser;
    this.#options = options;
  }

  start(): void {
    if (this.#stopped || this.#timer !== undefined) return;
    this.#timer = setInterval(() => { void this.#tick(); }, this.#options.intervalMs);
    this.#timer.unref();
    void this.#tick();
  }

  stop(): void {
    if (this.#stopped) return;
    this.#stopped = true;
    this.#controller.abort();
    if (this.#timer !== undefined) {
      clearInterval(this.#timer);
      this.#timer = undefined;
    }
  }

  async #tick(): Promise<void> {
    if (this.#running || this.#stopped) return;
    this.#running = true;
    try {
      const tabs = await this.#browser.tabs({ signal: this.#controller.signal });
      const ids = new Set(tabs.map(tab => tab.id));
      for (const id of this.#pages.keys()) {
        if (!ids.has(id)) {
          this.#pages.delete(id);
          this.#solving.delete(id);
        }
      }
      for (const id of ids) {
        if (this.#stopped) break;
        await this.#check(id);
      }
    } catch (error) {
      if (!this.#stopped) this.#notifyError(error);
    } finally {
      this.#running = false;
    }
  }

  async #check(id: string): Promise<void> {
    if (this.#solving.has(id)) return;
    let page = this.#pages.get(id);
    if (page === undefined) {
      page = this.#browser.page(id);
      this.#pages.set(id, page);
    }
    let title: string;
    try {
      title = await page.title({ signal: this.#controller.signal });
    } catch (error) {
      if (!this.#stopped) this.#notifyError(error, page);
      return;
    }
    if (this.#stopped || !isCloudflareChallengeTitle(title)) return;
    this.#solving.add(id);
    this.#notify('onChallenge', page);
    try {
      await page.solveTurnstile({ timeoutMs: this.#options.timeoutMs, signal: this.#controller.signal });
      if (!this.#stopped) this.#notify('onSolved', page);
    } catch (error) {
      if (!this.#stopped) this.#notifyError(error, page);
    } finally {
      this.#solving.delete(id);
    }
  }

  #notify(name: 'onChallenge' | 'onSolved', page: Page): void {
    const callback = this.#options[name];
    if (callback === undefined) return;
    try {
      callback(page);
    } catch (error) {
      this.#notifyError(error, page);
    }
  }

  #notifyError(error: unknown, page?: Page): void {
    const callback = this.#options.onError;
    if (callback === undefined) return;
    try {
      callback(error, page);
    } catch {
      // A failing observer must not stop the watcher.
    }
  }
}
