// One release version for every crate and SDK: [workspace.package] version in
// the root Cargo.toml. Every file below repeats it; scripts/bump-release.mjs
// writes them and versionDrift() checks them. The eoka core pin in
// crates/eoka-server/Cargo.toml follows the workspace eoka dependency instead.
import { readFile, writeFile } from 'node:fs/promises';
import { join, resolve } from 'node:path';

const repoRoot = resolve(import.meta.dirname, '../../..');
const read = path => readFile(join(repoRoot, path), 'utf8');
const crates = ['eoka-solvers', 'eoka-email', 'eoka-protocol', 'eoka-sdk', 'eoka-server', 'eoka-tack'];

export function workspaceDependencyVersion(cargoToml, name) {
  const section = cargoToml.split(/^\[workspace\.dependencies\]$/m)[1]?.split(/^\[/m)[0] ?? '';
  const line = new RegExp(`^${name} = (.+)$`, 'm').exec(section)?.[1] ?? '';
  const version = /^"([^"]+)"$/.exec(line)?.[1] ?? /\bversion = "([^"]+)"/.exec(line)?.[1];
  if (!version) throw new Error(`Cargo.toml [workspace.dependencies] has no version for ${name}`);
  return version;
}

export async function releaseVersion() {
  const section = (await read('Cargo.toml')).split(/^\[workspace\.package\]$/m)[1]?.split(/^\[/m)[0] ?? '';
  const version = /^version = "([^"]+)"$/m.exec(section)?.[1];
  if (!version) throw new Error('Cargo.toml [workspace.package] has no version');
  return version;
}

// Each pin: a file and a pattern whose second group is the repeated version.
const pins = version => [
  ...crates.map(name => ({ file: 'Cargo.toml', pattern: new RegExp(`^(${name} = \\{ version = ")([^"]+)(")`, 'gm') })),
  { file: 'clients/node/package.json', fields: [[]] },
  { file: 'clients/node/package-lock.json', fields: [[], ['packages', '']] },
  { file: 'clients/node/release-manifest.json', pattern: /("(?:sdkVersion|packageVersion)": ")([^"]+)(")/g },
  { file: 'clients/node/release-manifest.json', pattern: /("server": \{\n {4}"version": ")([^"]+)(")/g },
  { file: 'clients/go/download.go', pattern: /^(const serverReleaseVersion = ")([^"]+)(")$/gm },
  { file: 'clients/go/README.md', pattern: /(eoka-server-v)(\d+\.\d+\.\d+[^/)\]`\s]*)()/g },
  { file: 'crates/eoka-cli/skill/SKILL.md', pattern: /(\*\*)(\d+\.\d+)( \/ protocol)/g, minor: true },
];
const expected = (pin, version) => pin.minor ? version.split('.').slice(0, 2).join('.') : version;
const at = (value, path) => path.reduce((node, key) => node?.[key], value);
async function found(pin) {
  const text = await read(pin.file);
  if (pin.fields) return pin.fields.map(path => at(JSON.parse(text), path)?.version).filter(Boolean);
  return [...text.matchAll(pin.pattern)].map(match => match[2]);
}

export async function versionDrift() {
  const version = await releaseVersion();
  const drift = [];
  for (const pin of pins(version)) {
    const values = await found(pin);
    if (!values.length) drift.push(`${pin.file} no longer contains the release version`);
    for (const value of values) if (value !== expected(pin, version)) drift.push(`${pin.file} has ${value}, the release version is ${expected(pin, version)}`);
  }
  const cargo = await read('Cargo.toml');
  const core = workspaceDependencyVersion(cargo, 'eoka');
  const serverPin = /^eoka = "=([^"]+)"$/m.exec(await read('crates/eoka-server/Cargo.toml'))?.[1];
  if (serverPin !== core) drift.push(`crates/eoka-server/Cargo.toml pins eoka =${serverPin}, the workspace uses ${core}`);
  return drift;
}

export async function writeVersion(version) {
  const cargo = await read('Cargo.toml');
  const section = /^(\[workspace\.package\]\n(?:[^[\n].*\n)*?)version = "[^"]+"/m;
  if (!section.test(cargo)) throw new Error('Cargo.toml [workspace.package] has no version line');
  await writeFile(join(repoRoot, 'Cargo.toml'), cargo.replace(section, `$1version = "${version}"`));
  for (const pin of pins(version)) {
    const text = await read(pin.file);
    if (pin.fields) {
      const value = JSON.parse(text);
      for (const path of pin.fields) at(value, path).version = version;
      await writeFile(join(repoRoot, pin.file), `${JSON.stringify(value, null, 2)}\n`);
    } else {
      await writeFile(join(repoRoot, pin.file), text.replace(pin.pattern, (_, before, _old, after) => `${before}${expected(pin, version)}${after}`));
    }
  }
  const core = workspaceDependencyVersion(await read('Cargo.toml'), 'eoka');
  const server = await read('crates/eoka-server/Cargo.toml');
  await writeFile(join(repoRoot, 'crates/eoka-server/Cargo.toml'), server.replace(/^eoka = "=[^"]+"$/m, `eoka = "=${core}"`));
}
