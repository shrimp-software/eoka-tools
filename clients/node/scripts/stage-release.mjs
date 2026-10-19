import { createHash } from 'node:crypto';
import { createReadStream } from 'node:fs';
import { chmod, copyFile, cp, lstat, mkdir, mkdtemp, open, readFile, rename, rm, writeFile } from 'node:fs/promises';
import { execFile } from 'node:child_process';
import { dirname, join, resolve } from 'node:path';
import { promisify } from 'node:util';
import { detectPlatform, platformIds } from '../dist/platform.js';
import { validateReleaseManifest } from '../dist/release.js';

const exec = promisify(execFile);
const sdkRoot = resolve(import.meta.dirname, '..');

export async function stageRelease({ artifacts, output, hostOnly = false, publication = false, platforms = platformIds, root = sdkRoot }) {
  artifacts = resolve(artifacts);
  output = resolve(output);
  const manifest = structuredClone(validateReleaseManifest(JSON.parse(await readFile(join(root, 'release-manifest.json'), 'utf8'))));
  const selected = hostOnly ? [detectPlatform()] : [...platforms];
  if (!hostOnly && (selected.length !== platformIds.length || !platformIds.every((id) => selected.includes(id)))) throw new Error('Release staging requires all five server artifacts. Use --host-only for a clearly marked local development gate.');
  if (publication && hostOnly) throw new Error('Publication cannot use host-only development artifacts');
  if (publication && Object.keys(process.env).some(key => /^(?:EOKA_(?:SERVER_BIN|CHROME_PATH|CORE_(?:REF|PATH))|CARGO_PATCH_|CARGO_SOURCE_|CARGO_BUILD_RUSTFLAGS$|RUSTFLAGS$)/.test(key) && process.env[key])) throw new Error('Publication cannot use local runtime or Cargo overrides');
  if (publication && (manifest.sdkVersion !== '0.1.0-beta.6' || manifest.server.version !== '0.6.3' || manifest.server.protocolVersion !== '1.3' || Object.values(manifest.server.platforms).some(pin => pin.development))) throw new Error('Publication requires reserved prerelease versions and non-development source metadata');
  if (publication) {
    const readiness = JSON.parse(await readFile(join(root, 'scripts', 'publication-readiness.json'), 'utf8'));
    if (readiness.schemaVersion !== 1 || readiness.sdkVersion !== manifest.sdkVersion || readiness.chromeBuildId !== manifest.chrome.buildId || readiness.status !== 'ready' || !Array.isArray(readiness.blockers) || readiness.blockers.length !== 0) throw new Error(`Publication readiness blocked: ${JSON.stringify(readiness.blockers)}`);
  }
  try {
    await lstat(output);
    throw new Error(`Refusing to replace existing staging output: ${output}`);
  } catch (cause) {
    if (cause.code !== 'ENOENT') throw cause;
  }
  await mkdir(dirname(output), { recursive: true });
  const staging = await mkdtemp(join(dirname(output), '.eoka-release-'));
  try {
    for (const id of platformIds) {
      const pin = manifest.server.platforms[id];
      pin.sha256 = null;
      pin.size = null;
      pin.minimumGlibc = id.startsWith('linux-') ? '2.35' : null;
      pin.development = false;
    }
    const packageMetadata = JSON.parse(await readFile(join(root, 'package.json'), 'utf8'));
    if (packageMetadata.version !== manifest.sdkVersion || packageMetadata.name !== '@cbxss/eoka') throw new Error('SDK package version does not match source release manifest');
    if (publication && packageMetadata.private !== true) throw new Error('Publication staging requires a private source SDK');
    packageMetadata.optionalDependencies = Object.fromEntries(
      Object.values(manifest.server.platforms).map(pin => [pin.packageName, pin.packageVersion]),
    );
    const nativePackages = [];
    for (const id of selected) {
      const pin = manifest.server.platforms[id];
      const source = join(artifacts, pin.asset);
      if (!(await lstat(source)).isFile()) throw new Error(`Artifact must be a regular file, not a symlink: ${source}`);
      const directory = join(staging, 'platforms', id);
      await mkdir(join(directory, 'bin'), { recursive: true });
      const binaryPath = join(directory, pin.executablePath);
      await copyFile(source, binaryPath);
      await chmod(binaryPath, 0o755);
      const file = await lstat(binaryPath);
      if (file.size < 64) throw new Error(`Artifact must be a real native binary: ${source}`);
      const handle = await open(binaryPath, 'r');
      let header;
      try {
        header = Buffer.alloc(64);
        await handle.read(header, 0, 64, 0);
        if (id.startsWith('linux-')) {
          const machine = id === 'linux-x64-gnu' ? 62 : 183;
          if (!header.subarray(0, 4).equals(Buffer.from([0x7f, 0x45, 0x4c, 0x46])) || header[4] !== 2 || header[5] !== 1 || header.readUInt16LE(18) !== machine) throw new Error(`ELF architecture mismatch: ${id}`);
        } else if (id.startsWith('darwin-')) {
          if (header.readUInt32LE(0) !== 0xfeedfacf || header.readUInt32LE(4) !== (id === 'darwin-x64' ? 0x01000007 : 0x0100000c)) throw new Error(`Mach-O architecture mismatch: ${id}`);
        } else {
          if (header.toString('ascii', 0, 2) !== 'MZ') throw new Error('Windows artifact is not PE');
          const pe = Buffer.alloc(6);
          await handle.read(pe, 0, 6, header.readUInt32LE(60));
          if (pe.readUInt32LE(0) !== 0x00004550 || pe.readUInt16LE(4) !== 0x8664) throw new Error('PE architecture mismatch: win32-x64');
        }
      } finally {
        await handle.close();
      }
      if (id.startsWith('linux-')) {
        const { stdout } = await exec('readelf', ['--version-info', '--wide', binaryPath], { maxBuffer: 8 * 1024 * 1024 });
        let maximumMajor = stdout.includes('GLIBC_ABI_DT_RELR') ? 2 : -1;
        let maximumMinor = maximumMajor === 2 ? 36 : 0;
        for (const match of stdout.matchAll(/\bGLIBC_(\d+)\.(\d+)\b/g)) {
          const major = Number(match[1]);
          const minor = Number(match[2]);
          if (major > maximumMajor || (major === maximumMajor && minor > maximumMinor)) {
            maximumMajor = major;
            maximumMinor = minor;
          }
        }
        if (maximumMajor < 0) throw new Error(`Cannot establish GNU libc ABI floor: ${source}`);
        const aboveBaseline = maximumMajor > 2 || (maximumMajor === 2 && maximumMinor > 35);
        const minimumGlibc = aboveBaseline ? `${maximumMajor}.${maximumMinor}` : '2.35';
        if (aboveBaseline && !hostOnly) throw new Error(`${id} requires glibc ${minimumGlibc}, above release baseline 2.35. Rebuild against Ubuntu 22.04; --host-only is development-only.`);
        pin.minimumGlibc = minimumGlibc;
      }
      pin.development = hostOnly;
      pin.size = file.size;
      const hash = createHash('sha256');
      for await (const chunk of createReadStream(binaryPath)) hash.update(chunk);
      pin.sha256 = hash.digest('hex');
      const [os, cpu] = id.split('-');
      const metadata = {
        schemaVersion: 1, sdkVersion: manifest.sdkVersion, packageName: pin.packageName, packageVersion: pin.packageVersion,
        platform: id, target: pin.target, serverVersion: manifest.server.version, protocolVersion: manifest.server.protocolVersion,
        minimumGlibc: pin.minimumGlibc, development: pin.development, binary: { path: pin.executablePath, sha256: pin.sha256, size: pin.size },
      };
      const npmPackage = {
        name: pin.packageName, version: pin.packageVersion, private: !publication, description: `Pinned Eoka server for ${id}${hostOnly ? ' (local host-development artifact)' : ''}`,
        license: 'MIT', os: [os], cpu: [cpu], ...(os === 'linux' ? { libc: ['glibc'] } : {}),
        files: ['bin/', 'release-metadata.json', 'LICENSE'], exports: { './package.json': './package.json', './release-metadata.json': './release-metadata.json' },
        repository: { type: 'git', url: 'git+https://github.com/shrimp-software/eoka-tools.git', directory: 'clients/node' },
        engines: packageMetadata.engines,
      };
      nativePackages.push({ directory, npmPackage });
      await writeFile(join(directory, 'release-metadata.json'), `${JSON.stringify(metadata, null, 2)}\n`);
      await copyFile(new URL('../../../LICENSE', import.meta.url), join(directory, 'LICENSE'));
    }
    validateReleaseManifest(manifest);
    if (publication && Object.values(manifest.server.platforms).some(pin => pin.development || !/^[a-f0-9]{64}$/.test(pin.sha256 ?? '') || !Number.isSafeInteger(pin.size) || pin.size < 64 || (pin.target.includes('linux') && pin.minimumGlibc !== '2.35'))) throw new Error('Publication requires five fully prepared native artifacts');
    for (const { directory, npmPackage } of nativePackages) {
      await writeFile(join(directory, 'package.json'), `${JSON.stringify(npmPackage, null, 2)}\n`);
    }
    packageMetadata.private = !publication;
    packageMetadata.repository = { type: 'git', url: 'git+https://github.com/shrimp-software/eoka-tools.git', directory: 'clients/node' };
    const sdk = join(staging, 'sdk');
    await mkdir(sdk);
    await cp(join(root, 'dist'), join(sdk, 'dist'), { recursive: true });
    await writeFile(join(sdk, 'package.json'), `${JSON.stringify(packageMetadata, null, 2)}\n`);
    await copyFile(join(root, 'README.md'), join(sdk, 'README.md'));
    await cp(join(root, 'examples'), join(sdk, 'examples'), { recursive: true });
    const serialized = `${JSON.stringify(manifest, null, 2)}\n`;
    await writeFile(join(sdk, 'dist', 'release-manifest.json'), serialized);
    await writeFile(join(sdk, 'release-manifest.json'), serialized);
    await rename(staging, output);
    return { output, platforms: selected, development: hostOnly, publication };
  } catch (cause) {
    await rm(staging, { recursive: true, force: true });
    throw cause;
  }
}

if (process.argv[1] && resolve(process.argv[1]) === import.meta.filename) {
  const args = process.argv.slice(2);
  let artifacts;
  let output;
  let hostOnly = false;
  let publication = false;
  const platforms = [];
  for (let index = 0; index < args.length; index++) {
    const arg = args[index];
    if (arg === '--host-only') hostOnly = true;
    else if (arg === '--publication') publication = true;
    else if (['--artifacts', '--output', '--platform'].includes(arg)) {
      const value = args[++index];
      if (!value || value.startsWith('--')) throw new Error(`Missing value for ${arg}`);
      if (arg === '--artifacts') artifacts = value;
      else if (arg === '--output') output = value;
      else platforms.push(value);
    } else throw new Error(`Unknown argument: ${arg}`);
  }
  if (!artifacts || !output || platforms.some((id) => !platformIds.includes(id)) || (hostOnly && (platforms.length || publication))) throw new Error('Usage: node scripts/stage-release.mjs --artifacts DIR --output NEW_DIR [--publication] [--host-only | --platform ID (all five)]');
  const result = await stageRelease({ artifacts, output, hostOnly, publication, ...(platforms.length ? { platforms } : {}) });
  process.stdout.write(`${JSON.stringify(result)}\n`);
}
