import { createHash } from 'node:crypto';
import { createReadStream, constants } from 'node:fs';
import { access, lstat, readFile, readdir, readlink, realpath, writeFile } from 'node:fs/promises';
import path from 'node:path';
import { InstallationError } from './install-errors.js';
import type { ChromePlatformRelease } from './release.js';

export const markerName = '.eoka-installation.json';
export interface InventoryEntry {
  path: string;
  type: 'file' | 'directory' | 'link';
  mode: number;
  size?: number;
  sha256?: string;
  target?: string;
}
export interface InstallationMarker {
  schema: 1;
  buildId: string;
  platform: string;
  archiveSha256: string;
  archiveUrl: string;
  executablePath: string;
  inventorySha256: string;
  inventory: InventoryEntry[];
}

export function fingerprint(entries: InventoryEntry[]): string {
  return createHash('sha256').update(JSON.stringify(entries)).digest('hex');
}

function inside(root: string, target: string): boolean {
  const relative = path.relative(root, target);
  return relative === '' || (!relative.startsWith(`..${path.sep}`) && relative !== '..' && !path.isAbsolute(relative));
}

export interface InventoryOptions {
  /**
   * Root-relative, `/`-separated directory to skip while it is empty. Chrome
   * creates `extensions/` next to its executable on every startup
   * (component-extension staging) even with `--disable-component-update`; it
   * is not part of the pinned archive, so it must not read as tampering.
   */
  ignoreEmptyDirectory?: string;
}

export async function inventoryTree(root: string, options: InventoryOptions = {}): Promise<InventoryEntry[]> {
  const rootInfo = await lstat(root);
  if (!rootInfo.isDirectory() || rootInfo.isSymbolicLink()) throw new InstallationError('IntegrityMismatch', 'Browser installation root is not a regular directory');
  const resolvedRoot = await realpath(root);
  const entries: InventoryEntry[] = [];
  async function visit(directory: string, prefix: string): Promise<void> {
    const names = (await readdir(directory)).sort();
    for (const name of names) {
      if (!prefix && name === markerName) continue;
      const relative = prefix ? `${prefix}/${name}` : name;
      const absolute = path.join(directory, name);
      const info = await lstat(absolute);
      const mode = info.mode & 0o555;
      if (relative === options.ignoreEmptyDirectory && info.isDirectory() && (await readdir(absolute)).length === 0) continue;
      if (info.isSymbolicLink()) {
        const target = await readlink(absolute);
        try {
          if (path.isAbsolute(target) || !inside(resolvedRoot, await realpath(absolute))) throw new Error('Link escapes browser distribution');
        } catch (cause) {
          throw new InstallationError('IntegrityMismatch', `Unsafe or broken browser link: ${relative}`, { cause });
        }
        entries.push({ path: relative, type: 'link', mode, target });
      } else if (info.isDirectory()) {
        entries.push({ path: relative, type: 'directory', mode });
        await visit(absolute, relative);
      } else if (info.isFile()) {
        const hash = createHash('sha256');
        for await (const chunk of createReadStream(absolute)) hash.update(chunk);
        entries.push({ path: relative, type: 'file', mode, size: info.size, sha256: hash.digest('hex') });
      } else {
        throw new InstallationError('IntegrityMismatch', `Unsupported browser resource: ${relative}`);
      }
    }
  }
  await visit(root, '');
  return entries;
}

export async function validateExecutable(root: string, relative: string): Promise<void> {
  const absolute = path.resolve(root, relative);
  if (!inside(root, absolute)) throw new InstallationError('IntegrityMismatch', 'Browser executable escapes the installation');
  const info = await lstat(absolute);
  if (!info.isFile() || info.isSymbolicLink() || info.size === 0) throw new InstallationError('IntegrityMismatch', 'Browser executable is missing or unsafe');
  await access(absolute, process.platform === 'win32' ? constants.R_OK : constants.R_OK | constants.X_OK);
}

export async function createMarker(root: string, buildId: string, platform: string, pin: ChromePlatformRelease, executablePath: string): Promise<void> {
  await validateExecutable(root, executablePath);
  const inventory = await inventoryTree(root);
  const marker: InstallationMarker = { schema: 1, buildId, platform, archiveSha256: pin.sha256!, archiveUrl: pin.url, executablePath, inventorySha256: fingerprint(inventory), inventory };
  await writeFile(path.join(root, markerName), JSON.stringify(marker), { flag: 'wx', mode: 0o600 });
}

export async function verifyInstallation(root: string, buildId: string, platform: string, pin: ChromePlatformRelease, executablePath: string): Promise<boolean> {
  try {
    await lstat(root);
  } catch (error) {
    if ((error as NodeJS.ErrnoException).code === 'ENOENT') return false;
    throw error;
  }
  try {
    const markerPath = path.join(root, markerName);
    const markerInfo = await lstat(markerPath);
    if (!markerInfo.isFile() || markerInfo.isSymbolicLink() || markerInfo.size > 64 * 1024 * 1024) throw new Error('Invalid installation inventory');
    const marker = JSON.parse(await readFile(markerPath, 'utf8')) as InstallationMarker;
    if (marker.schema !== 1 || marker.buildId !== buildId || marker.platform !== platform || marker.archiveSha256 !== pin.sha256 || marker.archiveUrl !== pin.url || marker.executablePath !== executablePath || !Array.isArray(marker.inventory) || marker.inventorySha256 !== fingerprint(marker.inventory)) throw new Error('Installation inventory does not match the pinned release');
    await validateExecutable(root, executablePath);
    // The marker is always written from a strict inventory (createMarker).
    // Chrome's archive never ships `extensions/`; the leniency only covers the
    // empty directory it creates beside the executable after the first launch.
    const runtimeExtensions = path.posix.join(path.posix.dirname(executablePath), 'extensions');
    const actual = await inventoryTree(root, { ignoreEmptyDirectory: runtimeExtensions });
    if (fingerprint(actual) !== marker.inventorySha256) throw new Error('Browser distribution resources have changed');
    return true;
  } catch (cause) {
    if (cause instanceof InstallationError) throw cause;
    throw new InstallationError('IntegrityMismatch', `Browser cache failed integrity verification: ${root}`, { cause });
  }
}
