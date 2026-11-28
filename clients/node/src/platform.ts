import { spawn } from 'node:child_process';
import { join } from 'node:path';
import { InstallationError } from './install-errors.js';

export const platformIds = ['linux-x64-gnu', 'linux-arm64-gnu', 'darwin-x64', 'darwin-arm64', 'win32-x64'] as const;
export type PlatformId = typeof platformIds[number];

export function selectPlatform(os: string, arch: string, glibcRuntimeVersion?: string): PlatformId {
  if (os === 'linux' && (arch === 'x64' || arch === 'arm64')) {
    const version = glibcRuntimeVersion?.match(/^(\d+)\.(\d+)(?:\.\d+)?$/);
    if (!version) {
      throw new InstallationError('UnsupportedPlatform', 'Managed Eoka requires GNU/Linux with glibc >= 2.35; musl/Alpine and unidentified libc are unsupported. Use explicit serverPath and executablePath only as an expert override.');
    }
    const major = Number(version[1]);
    const minor = Number(version[2]);
    if (major < 2 || (major === 2 && minor < 35)) {
      throw new InstallationError('UnsupportedPlatform', `Managed Eoka requires glibc >= 2.35; detected ${glibcRuntimeVersion}. Upgrade the OS or supply both binaries explicitly.`);
    }
    return arch === 'x64' ? 'linux-x64-gnu' : 'linux-arm64-gnu';
  }
  if (os === 'darwin' && arch === 'x64') return 'darwin-x64';
  if (os === 'darwin' && arch === 'arm64') return 'darwin-arm64';
  if (os === 'win32' && arch === 'x64') return 'win32-x64';
  throw new InstallationError('UnsupportedPlatform', `Managed Eoka does not support ${os}/${arch}. Supported platforms: ${platformIds.join(', ')}. Supply both binaries explicitly only as an expert override.`);
}

export function detectPlatform(): PlatformId {
  const report = process.platform === 'linux' ? process.report.getReport() as { header: { glibcVersionRuntime?: string } } : undefined;
  return selectPlatform(process.platform, process.arch, report?.header.glibcVersionRuntime);
}

function missing(cause: unknown): boolean {
  return (cause as NodeJS.ErrnoException | undefined)?.code === 'ESRCH';
}

function taskkill(pid: number): Promise<boolean> {
  const root = process.env.SystemRoot ?? process.env.WINDIR;
  if (!root) return Promise.reject(new Error('Windows SystemRoot is required for owned-process-tree termination'));
  return new Promise<boolean>((resolve, reject) => {
    const killer = spawn(join(root, 'System32', 'taskkill.exe'), ['/PID', String(pid), '/T', '/F'], { shell: false, stdio: 'ignore', windowsHide: true });
    const timer = setTimeout(() => {
      try {
        killer.kill('SIGKILL');
      } catch (cause) {
        reject(cause);
        return;
      }
      reject(new Error('Owned-process-tree termination command exceeded its deadline'));
    }, 1000);
    killer.once('error', cause => {
      clearTimeout(timer);
      reject(cause);
    });
    killer.once('close', code => {
      clearTimeout(timer);
      if (code === 0) resolve(true);
      else reject(new Error(`Owned-process-tree termination command exited with code ${code}`));
    });
  });
}

export const processTree = {
  exists(pid: number): boolean {
    try {
      process.kill(pid, 0);
      return true;
    } catch (cause) {
      return !missing(cause);
    }
  },
  async signal(pid: number, force: boolean): Promise<boolean> {
    if (process.platform === 'win32') return taskkill(pid);
    try {
      process.kill(-pid, force ? 'SIGKILL' : 'SIGTERM');
      return true;
    } catch (cause) {
      if (missing(cause)) return false;
      throw cause;
    }
  },
};
