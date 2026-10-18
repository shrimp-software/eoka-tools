import { createHash } from 'node:crypto';
import { constants, createReadStream } from 'node:fs';
import { access, lstat, readFile, realpath, stat } from 'node:fs/promises';
import { createRequire } from 'node:module';
import { dirname, join, relative, resolve, sep } from 'node:path';
import { ConfigurationError } from './errors.js';
import { InstallationError } from './install-errors.js';
import { detectPlatform } from './platform.js';
import { releaseManifest } from './release.js';

export interface ServerBinaryOptions {
  serverPath?: string;
  env?: NodeJS.ProcessEnv;
  signal?: AbortSignal;
}

export interface ServerBinaryResolution {
  executablePath: string;
  source: 'explicit' | 'environment' | 'package';
  expectedServerVersion?: string;
}

function checkAbort(signal?: AbortSignal): void {
  if (signal?.aborted) throw new InstallationError('InstallationAborted', 'Server resolution was aborted.', { cause: signal.reason });
}

function isSingleton(value: unknown, expected: string): boolean {
  return Array.isArray(value) && value.length === 1 && value[0] === expected;
}

// npm skips an optional dependency whose engines range rejects the running
// Node, so a missing platform package often means the wrong Node version.
export function nodeEngineMismatch(range: string, version: string): boolean {
  const required = /^\^(\d+)\.(\d+)\.(\d+)$/.exec(range)?.slice(1).map(Number);
  const current = /^v?(\d+)\.(\d+)\.(\d+)/.exec(version)?.slice(1).map(Number);
  if (!required || !current || required[0] === 0) return false;
  if (current[0] !== required[0]) return true;
  return current[1]! < required[1]! || (current[1] === required[1] && current[2]! < required[2]!);
}

function unsupportedNodeHint(): string {
  let range: unknown;
  try {
    range = (createRequire(import.meta.url)('../package.json') as { engines?: { node?: unknown } }).engines?.node;
  } catch {
    return '';
  }
  if (typeof range !== 'string' || !nodeEngineMismatch(range, process.versions.node)) return '';
  return ` npm skips it on unsupported Node versions: this is Node ${process.versions.node}, but @cbxss/eoka requires ${range}. Switch Node (for example \`mise use node@24\` or \`nvm install 24\`) and reinstall.`;
}

export async function resolveServerBinary(options: ServerBinaryOptions = {}): Promise<ServerBinaryResolution> {
  const { signal } = options;
  checkAbort(signal);
  const env = options.env ?? process.env;
  const override = options.serverPath ?? env.EOKA_SERVER_BIN;
  if (override !== undefined) {
    if (typeof override !== 'string' || override.trim() === '' || override.includes('\0')) throw new ConfigurationError('serverPath or EOKA_SERVER_BIN must be a nonempty filesystem path');
    const executablePath = resolve(override);
    try {
      if (!(await stat(executablePath)).isFile()) throw new Error('not a regular file');
      await access(executablePath, process.platform === 'win32' ? constants.F_OK : constants.X_OK);
    } catch (cause) {
      throw new ConfigurationError(`serverPath is not an accessible executable: ${executablePath}`, { cause });
    }
    checkAbort(signal);
    return { executablePath, source: options.serverPath === undefined ? 'environment' : 'explicit' };
  }
  const platform = detectPlatform();
  const pin = releaseManifest.server.platforms[platform];
  let packagePath: string;
  try {
    packagePath = createRequire(import.meta.url).resolve(`${pin.packageName}/package.json`);
  } catch (cause) {
    throw new InstallationError('ServerPackageMissing', `The exact optional dependency ${pin.packageName}@${pin.packageVersion} is missing.${unsupportedNodeHint()} Reinstall @cbxss/eoka with optional dependencies enabled (do not use --omit=optional), or install that exact platform package. There is no Cargo, PATH or network fallback.`, { cause });
  }
  if (pin.sha256 === null || pin.size === null) throw new InstallationError('ReleaseNotPrepared', `The SDK has no staged server integrity pin for ${platform}. Use a prepared SDK plus its matching platform package, or set serverPath explicitly.`);
  if (pin.minimumGlibc !== null && pin.minimumGlibc !== '2.35') {
    const report = process.report.getReport() as { header: { glibcVersionRuntime?: string } };
    const current = report.header.glibcVersionRuntime?.split('.').map(Number);
    const minimum = pin.minimumGlibc.split('.').map(Number);
    if (!current || current[0]! < minimum[0]! || (current[0] === minimum[0] && current[1]! < minimum[1]!)) throw new InstallationError('UnsupportedPlatform', `This ${pin.development ? 'host-development' : 'release'} server requires glibc >= ${pin.minimumGlibc}. The SDK never substitutes another binary.`);
  }
  try {
    const packageRoot = await realpath(dirname(packagePath));
    const packageMetadata = JSON.parse(await readFile(packagePath, { encoding: 'utf8', ...(signal === undefined ? {} : { signal }) })) as Record<string, unknown>;
    const [os, arch] = platform.split('-');
    if (packageMetadata.name !== pin.packageName || packageMetadata.version !== pin.packageVersion || !isSingleton(packageMetadata.os, os!) || !isSingleton(packageMetadata.cpu, arch!) || (os === 'linux' && !isSingleton(packageMetadata.libc, 'glibc'))) throw new Error('Package name, exact version, OS, architecture or libc does not match the SDK pin');
    const metadataPath = join(packageRoot, 'release-metadata.json');
    if (!(await lstat(metadataPath)).isFile()) throw new Error('Release metadata must be a regular file, not a symlink');
    const metadata = JSON.parse(await readFile(metadataPath, { encoding: 'utf8', ...(signal === undefined ? {} : { signal }) })) as Record<string, unknown>;
    const expectedMetadata: Record<string, unknown> = {
      schemaVersion: 1, sdkVersion: releaseManifest.sdkVersion, packageName: pin.packageName, packageVersion: pin.packageVersion,
      platform, target: pin.target, serverVersion: releaseManifest.server.version, protocolVersion: releaseManifest.server.protocolVersion,
      minimumGlibc: pin.minimumGlibc, development: pin.development,
    };
    for (const key of Object.keys(expectedMetadata)) {
      if (metadata[key] !== expectedMetadata[key]) throw new Error(`Release metadata mismatch: ${key}`);
    }
    const binary = metadata.binary as Record<string, unknown> | undefined;
    if (!binary || binary.path !== pin.executablePath || binary.sha256 !== pin.sha256 || binary.size !== pin.size) throw new Error('Binary release metadata does not match the SDK pin');
    const executablePath = join(packageRoot, pin.executablePath);
    let ancestor = dirname(executablePath);
    while (ancestor !== packageRoot) {
      if (!(await lstat(ancestor)).isDirectory()) throw new Error('Executable parent must be a directory, not a symlink');
      ancestor = dirname(ancestor);
    }
    const file = await lstat(executablePath);
    if (!file.isFile() || file.size !== pin.size || (process.platform !== 'win32' && ((file.mode & 0o111) === 0 || (file.mode & 0o022) !== 0))) throw new Error('Binary must be a regular executable with the pinned size and no group/world writable mode');
    const canonical = await realpath(executablePath);
    const contained = relative(packageRoot, canonical);
    if (contained.startsWith(`..${sep}`) || contained === '..' || resolve(packageRoot, contained) !== canonical) throw new Error('Binary escapes its platform package');
    await access(executablePath, process.platform === 'win32' ? constants.F_OK : constants.X_OK);
    const hash = createHash('sha256');
    const stream = createReadStream(executablePath, signal === undefined ? {} : { signal });
    for await (const chunk of stream) hash.update(chunk);
    checkAbort(signal);
    if (hash.digest('hex') !== pin.sha256) throw new InstallationError('IntegrityMismatch', `The server binary in ${pin.packageName} does not match the SDK's SHA-256 pin. Reinstall the exact SDK/platform release.`);
    return { executablePath, source: 'package', expectedServerVersion: releaseManifest.server.version };
  } catch (cause) {
    checkAbort(signal);
    if (cause instanceof InstallationError) throw cause;
    throw new InstallationError('ServerPackageMismatch', `The installed ${pin.packageName}@${pin.packageVersion} does not match this prepared SDK release. Reinstall both exact packages.`, { cause });
  }
}
