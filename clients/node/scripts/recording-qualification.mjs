import assert from 'node:assert/strict';

export const recordingSchedule = [
  { name: 'short-default-67', waitMs: 67 },
  { name: 'short-default-101', waitMs: 101 },
  { name: 'short-default-167', waitMs: 167 },
  { name: 'short-default-251', waitMs: 251 },
  { name: 'short-120fps-67', fps: 120, waitMs: 67 },
  { name: 'short-120fps-251', fps: 120, waitMs: 251 },
  { name: 'short-1fps-1250', fps: 1, waitMs: 1250 },
  { name: 'native-0', fps: 30, waitMs: 2000 },
  { name: 'native-1', fps: 30, waitMs: 2000 },
  { name: 'native-2', fps: 30, waitMs: 6500 },
];

export function classifyRecordingDiagnostics(recording) {
  const advisories = [];
  const integer = value => {
    if (!(typeof value === 'string' || Number.isSafeInteger(value)) || !/^-?\d+$/.test(String(value))) throw new Error('Missing or inexact packet timing for diagnostic verification');
    return BigInt(value);
  };
  for (const source of ['probe', 'decode']) {
    const stderr = recording[`${source}Stderr`];
    if (typeof stderr !== 'string') throw new Error(`Missing ${source} diagnostics`);
    for (const line of stderr.trim().split(/\r?\n/)) {
      if (!line) continue;
      const match = /^\[(?:in#0\/)?mov,mp4,m4a,3gp,3g2,mj2 @ (?:0x)?[0-9a-fA-F]+\] DTS discontinuity in stream 0: packet (\d+) with DTS (-?\d+), packet (\d+) with DTS (-?\d+)$/.exec(line);
      if (!match) throw new Error(`Unrecognized ${source} diagnostic: ${line}`);
      const previous = Number(match[1]);
      const next = Number(match[3]);
      if (!Number.isSafeInteger(previous) || !Number.isSafeInteger(next) || next !== previous + 1 || !Array.isArray(recording.packetDts) || !Array.isArray(recording.packetDurations) || next >= recording.packetDts.length) throw new Error('Diagnostic does not identify adjacent recorded packets');
      const previousDts = integer(recording.packetDts[previous]);
      const nextDts = integer(recording.packetDts[next]);
      const duration = integer(recording.packetDurations[previous]);
      if (previousDts !== BigInt(match[2]) || nextDts !== BigInt(match[4]) || duration <= 0n || previousDts + duration !== nextDts) throw new Error('DTS discontinuity is not explained by the declared positive packet duration');
      advisories.push({ source, kind: 'vfr-rate-estimation', previousPacket: previous, nextPacket: next });
    }
  }
  return advisories;
}

export function assertRecordingQualification(evidence, chromeBuildId) {
  assert.equal(evidence?.readiness?.chromeBuildId, chromeBuildId, 'Recording readiness belongs to another Chrome build');
  assert.equal(evidence?.checks?.chrome?.buildId, chromeBuildId, 'Installed Chrome does not match recording qualification');
  const recordings = evidence?.checks?.nativeRecordings;
  assert.match(recordings?.browserVersion ?? '', /^(?:HeadlessChrome|Chrome)\/\d+\.\d+\.\d+\.\d+$/);
  assert.equal(recordings.browserVersion.split('/')[1], chromeBuildId, 'Recording ran on another Chrome build');
  assert.equal(recordings.artifactKind, 'finalized SDK MP4');
  assert.equal(recordings.attemptsPerCase, 1, 'Recording retries cannot establish qualification');
  assert.deepEqual(recordings.schedule, recordingSchedule, 'Incomplete recording qualification schedule');
  assert.ok(Array.isArray(evidence.recordings), 'Missing native recording evidence');
  assert.equal(evidence.recordings.length, recordingSchedule.length, 'Missing or duplicated recording cases');
  for (const [index, scheduled] of recordingSchedule.entries()) {
    const actual = evidence.recordings[index];
    assert.equal(actual?.name, scheduled.name, 'Unexpected recording case');
    assert.equal(actual.waitMs, scheduled.waitMs);
    assert.equal(actual.fps, scheduled.fps);
    assert.equal(actual.status, 'passed', `Recording failed: ${scheduled.name}`);
    assert.equal(actual.record?.state, 'completed', 'Recording was not published');
    assert.equal(actual.artifact?.container, 'mp4');
    assert.equal(actual.artifact?.mimeType, 'video/mp4');
    assert.deepEqual(actual.errors, [], 'Recording contains failure evidence');
    assert.deepEqual(actual.advisories, classifyRecordingDiagnostics(actual), 'Unverified recording diagnostic classification');
    assert.ok(Array.isArray(actual.packetDts));
    assert.equal(actual.sampleCount, actual.packetDts.length);
    assert.ok(actual.sampleCount >= (scheduled.waitMs >= 1000 ? 2 : 1), 'Insufficient recorded samples');
    let previous;
    for (const value of actual.packetDts) {
      assert.ok(typeof value === 'string' || Number.isSafeInteger(value), 'Inexact recorded decoding timestamp');
      assert.match(String(value), /^-?\d+$/);
      const dts = BigInt(value);
      assert.ok(previous === undefined || dts > previous, 'Non-increasing recorded decoding timestamps');
      previous = dts;
    }
  }
}
