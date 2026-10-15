import { spawn } from 'node:child_process';
import { Browser, install, getDownloadUrl, type BrowserPlatform } from '@puppeteer/browsers';
import path from 'node:path';
import { createMarker, verifyInstallation } from './browser-install-inventory.js';
import type { ChromePlatformRelease } from './release.js';
import { sanitizeInstallationDiagnostic } from './browser-install-diagnostics.js';

export interface WorkerRequest {
  action: 'verify' | 'install';
  root: string;
  buildId: string;
  platform: string;
  pin: ChromePlatformRelease;
  executablePath: string;
}
export type WorkerMessage = { type: 'result'; verified: boolean } | { type: 'progress'; downloadedBytes: number; totalBytes: number } | { type: 'error'; code: string; message: string };

process.once('disconnect', () => {
  if (process.platform === 'win32') {
    spawn('taskkill', ['/PID', String(process.pid), '/T', '/F'], { stdio: 'ignore', windowsHide: true }).once('error', () => process.exit(1));
  } else {
    try { process.kill(-process.pid, 'SIGKILL'); } catch { process.exit(1); }
  }
});

process.once('message', async (request: WorkerRequest) => {
  try {
    if (request.action === 'verify') {
      const verified = await verifyInstallation(request.root, request.buildId, request.platform, request.pin, request.executablePath);
      process.send?.({ type: 'result', verified } satisfies WorkerMessage);
    } else {
      if (process.env.NODE_TLS_REJECT_UNAUTHORIZED === '0') throw new Error('Browser downloads require TLS certificate verification; remove NODE_TLS_REJECT_UNAUTHORIZED=0');
      if (!request.pin.sha256 || !/^[a-f0-9]{64}$/.test(request.pin.sha256)) throw new Error('Missing archive checksum');
      const platform = request.pin.platform as BrowserPlatform;
      const url = new URL(request.pin.url);
      const baseUrl = new URL('../../', url).href.replace(/\/$/, '');
      if (getDownloadUrl(Browser.CHROME, platform, request.buildId, baseUrl).href !== url.href) throw new Error('Pinned URL does not match the Chrome distribution');
      const browser = await install({
        browser: Browser.CHROME,
        buildId: request.buildId,
        platform,
        cacheDir: request.root,
        baseUrl,
        expectedHash: request.pin.sha256,
        installDeps: false,
        downloadProgressCallback: (downloadedBytes, totalBytes) => {
          process.send?.({ type: 'progress', downloadedBytes, totalBytes } satisfies WorkerMessage);
        },
      });
      if (path.relative(request.root, browser.executablePath).replaceAll(path.sep, '/') !== request.executablePath) throw new Error('Installer returned an unexpected executable path');
      await createMarker(request.root, request.buildId, request.platform, request.pin, request.executablePath);
      process.send?.({ type: 'result', verified: true } satisfies WorkerMessage);
    }
    process.exitCode = 0;
  } catch (error) {
    const rawMessage = error instanceof Error ? error.message : String(error);
    const message = sanitizeInstallationDiagnostic(rawMessage, process.env);
    const code = error && typeof error === 'object' && 'name' in error && error.name === 'InstallationError' && 'code' in error && typeof error.code === 'string' ? error.code : /integrity check failed/i.test(rawMessage) ? 'IntegrityMismatch' : 'InstallationFailed';
    process.send?.({ type: 'error', code, message } satisfies WorkerMessage);
    process.exitCode = 1;
  } finally {
    process.removeAllListeners('disconnect');
    process.disconnect?.();
  }
});
