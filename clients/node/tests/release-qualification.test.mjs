import assert from 'node:assert/strict';
import { execFile } from 'node:child_process';
import { createHash } from 'node:crypto';
import { copyFile, mkdir, mkdtemp, readFile, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';
import { promisify } from 'node:util';
import { recordingSchedule } from '../scripts/recording-qualification.mjs';

const exec = promisify(execFile);
const platforms = ['linux-x64-gnu', 'linux-arm64-gnu', 'darwin-x64', 'darwin-arm64', 'win32-x64'];
const sourceSha = 'a'.repeat(40);
const save = (path, value) => writeFile(path, JSON.stringify(value));

async function fixture(run) {
  const root = await mkdtemp(join(tmpdir(), 'eoka-qualification-policy-'));
  try {
    for (const script of ['qualify-release.mjs', 'recording-qualification.mjs']) {
      await copyFile(new URL(`../scripts/${script}`, import.meta.url), join(root, script));
    }
    const manifest = JSON.parse(await readFile(new URL('../release-manifest.json', import.meta.url), 'utf8'));
    const readiness = { schemaVersion: 1, sdkVersion: manifest.sdkVersion, chromeBuildId: manifest.chrome.buildId, status: 'blocked', blockers: ['Authenticated qualification is pending'] };
    const packages = [];
    await mkdir(join(root, 'packages'));
    for (const name of ['@cbxss/eoka', ...platforms.map(platform => manifest.server.platforms[platform].packageName)]) {
      const filename = `${packages.length}.tgz`;
      const bytes = Buffer.from(`aggregate fixture ${name}`);
      await writeFile(join(root, 'packages', filename), bytes);
      packages.push({ name, version: manifest.sdkVersion, filename, sha256: createHash('sha256').update(bytes).digest('hex') });
    }
    await save(join(root, 'packages', 'packages.json'), { manifest, packages });
    const evidence = platforms.map(platform => ({
      schemaVersion: 1, platform, sourceSha, publication: false, readiness, status: 'passed', preview: false, development: false, sandbox: true, packages,
      checks: {
        chrome: { buildId: manifest.chrome.buildId },
        nativeRecordings: { artifactKind: 'finalized SDK MP4', browserVersion: `Chrome/${manifest.chrome.buildId}`, schedule: recordingSchedule, attemptsPerCase: 1 },
      },
      recordings: recordingSchedule.map(scheduled => ({
        ...scheduled, status: 'passed', record: { state: 'completed' }, artifact: { container: 'mp4', mimeType: 'video/mp4' },
        errors: [], probeStderr: '', decodeStderr: '', advisories: [], sampleCount: 3, packetDts: [0, 1, 2],
      })),
    }));
    let attempt = 0;
    const qualify = async (publication, mode = 'aggregate') => {
      await save(join(root, 'publication-readiness.json'), readiness);
      for (const item of evidence) {
        await mkdir(join(root, 'evidence', item.platform), { recursive: true });
        await save(join(root, 'evidence', item.platform, 'qualification.json'), item);
      }
      const output = join(root, `output-${attempt++}`);
      const env = { ...process.env, EOKA_SOURCE_SHA: sourceSha };
      delete env.EOKA_PUBLICATION;
      if (publication !== undefined) env.EOKA_PUBLICATION = String(publication);
      const args = mode === 'aggregate' ? ['--aggregate', '--evidence', join(root, 'evidence')] : ['--cache', join(root, 'cache')];
      await exec(process.execPath, [join(root, 'qualify-release.mjs'), '--packages', join(root, 'packages'), '--output', output, ...args], { env });
      return JSON.parse(await readFile(join(output, 'qualification.json'), 'utf8'));
    };
    await run({ root, readiness, evidence, qualify });
  } finally {
    await rm(root, { recursive: true, force: true });
  }
}

const rejects = (promise, pattern) => assert.rejects(promise, error => {
  assert.notEqual(error.code, 0);
  assert.match(error.stderr, pattern);
  return true;
});

test('PR qualification preserves release blockers and produces non-publication evidence', () => fixture(async ({ readiness, qualify }) => {
  for (const setting of [undefined, false]) {
    const result = await qualify(setting);
    assert.equal(result.publication, false);
    assert.deepEqual(result.readiness, readiness);
    assert.equal(result.evidence.length, 5);
    assert.equal(result.packages.length, 6);
  }
}));

test('publication rejects blocked readiness in both runtime and aggregate commands', () => fixture(async ({ qualify }) => {
  await rejects(qualify(true), /Publication readiness blocked/);
  await rejects(qualify(true, 'runtime'), /Publication readiness blocked/);
}));

test('a ready label cannot override outstanding publication blockers', () => fixture(async ({ readiness, qualify }) => {
  readiness.status = 'ready';
  await rejects(qualify(true), /Publication readiness blocked/);
}));

test('publication cannot reuse ordinary CI evidence even after release approval', () => fixture(async ({ readiness, qualify }) => {
  readiness.status = 'ready';
  readiness.blockers = [];
  await rejects(qualify(true), /Qualification mode does not match/);
}));

test('approved publication requires all five matching publication runs', () => fixture(async ({ readiness, evidence, qualify }) => {
  readiness.status = 'ready';
  readiness.blockers = [];
  for (const item of evidence) item.publication = true;
  const result = await qualify(true);
  assert.equal(result.publication, true);
  assert.deepEqual(result.readiness.blockers, []);
  assert.deepEqual(result.platforms, platforms);
}));

test('PR qualification still rejects real runtime and recording failures', () => fixture(async ({ evidence, qualify }) => {
  evidence[0].status = 'failed';
  await rejects(qualify(false), /failed/);
  evidence[0].status = 'passed';
  evidence[0].recordings[0].status = 'failed';
  await rejects(qualify(false), /Recording failed/);
}));

test('aggregation rejects evidence from another source or readiness state', () => fixture(async ({ evidence, qualify }) => {
  evidence[0].sourceSha = 'b'.repeat(40);
  await rejects(qualify(false), /AssertionError/);
  evidence[0].sourceSha = sourceSha;
  evidence[0].readiness = { ...evidence[0].readiness, blockers: [] };
  await rejects(qualify(false), /Qualification readiness does not match/);
}));

test('non-publication aggregation still verifies package bytes', () => fixture(async ({ root, qualify }) => {
  await writeFile(join(root, 'packages', '0.tgz'), 'corrupt');
  await rejects(qualify(false), /AssertionError/);
}));

test('invalid qualification mode cannot silently disable publication gates', () => fixture(async ({ qualify }) => {
  await rejects(qualify('TRUE'), /EOKA_PUBLICATION must be true or false/);
}));
