import { createBrowserInstaller } from './browser-install-engine.js';
import { detectPlatform, type PlatformId } from './platform.js';
import { releaseManifest } from './release.js';

export interface InstallProgress {
  downloadedBytes: number;
  totalBytes: number;
  buildId: string;
}

export interface InstallBrowserOptions {
  browserCacheDir?: string;
  download?: 'auto' | 'never';
  signal?: AbortSignal;
  timeoutMs?: number;
  onProgress?: (progress: InstallProgress) => void;
  env?: NodeJS.ProcessEnv;
}

export interface BrowserInstallation {
  executablePath: string;
  cacheDir: string;
  buildId: string;
  platform: PlatformId;
  source: 'cache' | 'download';
}

export async function installBrowser(options: InstallBrowserOptions = {}): Promise<BrowserInstallation> {
  const platform = detectPlatform();
  return await createBrowserInstaller({ platform, buildId: releaseManifest.chrome.buildId, pin: releaseManifest.chrome.platforms[platform] })(options);
}
