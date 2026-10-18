import { readFileSync } from 'node:fs';
import { protocolVersion } from './generated/protocol.js';
import { InstallationError } from './install-errors.js';
import { platformIds, type PlatformId } from './platform.js';

export interface ServerPlatformRelease {
  readonly packageName: string;
  readonly packageVersion: string;
  readonly target: string;
  readonly asset: string;
  readonly executablePath: string;
  readonly sha256: string | null;
  readonly size: number | null;
  readonly minimumGlibc: string | null;
  readonly development: boolean;
}

export interface ServerRelease {
  readonly version: string;
  readonly protocolVersion: string;
  readonly platforms: Readonly<Record<PlatformId, ServerPlatformRelease>>;
}

export interface ChromePlatformRelease {
  readonly platform: 'linux' | 'linux_arm' | 'mac' | 'mac_arm' | 'win64';
  readonly url: string;
  readonly sha256: string;
  readonly executablePath: string;
}

export interface ChromeRelease {
  readonly buildId: string;
  readonly platforms: Readonly<Record<PlatformId, ChromePlatformRelease>>;
}

export interface ReleaseManifest {
  readonly schemaVersion: 1;
  readonly sdkVersion: string;
  readonly installerVersion: string;
  readonly server: ServerRelease;
  readonly chrome: ChromeRelease;
}

const mappings = {
  'linux-x64-gnu': ['x86_64-unknown-linux-gnu', 'eoka-server-linux-amd64', 'linux', 'linux64'],
  'linux-arm64-gnu': ['aarch64-unknown-linux-gnu', 'eoka-server-linux-arm64', 'linux_arm', 'linux-arm64'],
  'darwin-x64': ['x86_64-apple-darwin', 'eoka-server-darwin-amd64', 'mac', 'mac-x64'],
  'darwin-arm64': ['aarch64-apple-darwin', 'eoka-server-darwin-arm64', 'mac_arm', 'mac-arm64'],
  'win32-x64': ['x86_64-pc-windows-msvc', 'eoka-server-windows-amd64.exe', 'win64', 'win64'],
} as const;

function object(value: unknown): Record<string, unknown> {
  if (typeof value !== 'object' || value === null || Array.isArray(value)) throw new Error('Expected release metadata object');
  return value as Record<string, unknown>;
}

const digestPattern = /^[a-f0-9]{64}$/;
const versionPattern = /^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)(?:-(?:0|[1-9]\d*|\d*[A-Za-z-][0-9A-Za-z-]*)(?:\.(?:0|[1-9]\d*|\d*[A-Za-z-][0-9A-Za-z-]*))*)?(?:\+[0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*)?(?![\s\S])/;

function exactNpmVersion(value: unknown): value is string {
  if (typeof value !== 'string' || value.length > 256) return false;
  const match = versionPattern.exec(value);
  return match !== null && Number.isSafeInteger(Number(match[1])) && Number.isSafeInteger(Number(match[2])) && Number.isSafeInteger(Number(match[3]));
}

export function validateReleaseManifest(value: unknown): ReleaseManifest {
  try {
    const manifest = object(value);
    const server = object(manifest.server);
    const chrome = object(manifest.chrome);
    const servers = object(server.platforms);
    const browsers = object(chrome.platforms);
    if (manifest.schemaVersion !== 1 || !exactNpmVersion(manifest.sdkVersion) || manifest.installerVersion !== '3.2.4' || server.version !== '0.6.1' || server.protocolVersion !== `${protocolVersion.major}.${protocolVersion.minor}` || chrome.buildId !== '155.0.8059.39') throw new Error('Incompatible release versions');
    if (Object.keys(servers).length !== platformIds.length || Object.keys(browsers).length !== platformIds.length) throw new Error('Incomplete platform mapping');
    for (const id of platformIds) {
      const pin = object(servers[id]);
      const browser = object(browsers[id]);
      const [target, asset, installerPlatform, chromePlatform] = mappings[id];
      if (pin.packageName !== `@cbxss/eoka-server-${id}` || pin.packageVersion !== manifest.sdkVersion || pin.target !== target || pin.asset !== asset || pin.executablePath !== (id === 'win32-x64' ? 'bin/eoka-server.exe' : 'bin/eoka-server')) throw new Error(`Invalid server mapping: ${id}`);
      if (!((pin.sha256 === null && pin.size === null) || (typeof pin.sha256 === 'string' && digestPattern.test(pin.sha256) && Number.isSafeInteger(pin.size) && (pin.size as number) > 0))) throw new Error(`Invalid server integrity pin: ${id}`);
      if (typeof pin.development !== 'boolean' || (id.startsWith('linux-') ? typeof pin.minimumGlibc !== 'string' || !/^\d+\.\d+$/.test(pin.minimumGlibc) : pin.minimumGlibc !== null)) throw new Error(`Invalid server ABI metadata: ${id}`);
      if (!pin.development && id.startsWith('linux-') && pin.minimumGlibc !== '2.35') throw new Error(`Invalid release glibc baseline: ${id}`);
      const expectedUrl = `https://storage.googleapis.com/chrome-for-testing-public/${chrome.buildId}/${chromePlatform}/chrome-${chromePlatform}.zip`;
      const expectedExecutable = id.startsWith('darwin-') ? `chrome-${chromePlatform}/Google Chrome for Testing.app/Contents/MacOS/Google Chrome for Testing` : `chrome-${chromePlatform}/chrome${id === 'win32-x64' ? '.exe' : ''}`;
      if (browser.platform !== installerPlatform || browser.url !== expectedUrl || typeof browser.sha256 !== 'string' || !digestPattern.test(browser.sha256) || browser.executablePath !== expectedExecutable) throw new Error(`Invalid Chrome pin: ${id}`);
    }
    return freeze(value as ReleaseManifest);
  } catch (cause) {
    throw new InstallationError('ReleaseNotPrepared', 'The SDK release manifest is malformed or incompatible. Reinstall a prepared SDK release.', { cause });
  }
}

function freeze<T>(value: T): T {
  if (value !== null && typeof value === 'object') {
    for (const child of Object.values(value)) freeze(child);
    Object.freeze(value);
  }
  return value;
}

export const releaseManifest: ReleaseManifest = (() => {
  try {
    return validateReleaseManifest(JSON.parse(readFileSync(new URL('./release-manifest.json', import.meta.url), 'utf8')));
  } catch (cause) {
    if (cause instanceof InstallationError) throw cause;
    throw new InstallationError('ReleaseNotPrepared', 'The SDK release manifest cannot be read. Reinstall a prepared SDK release.', { cause });
  }
})();
