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
    assert.equal(typeof actual.probeStderr, 'string');
    assert.equal(actual.probeStderr.trim(), '', 'Recording probe emitted diagnostics');
    assert.equal(typeof actual.decodeStderr, 'string');
    assert.equal(actual.decodeStderr.trim(), '', 'Recording decoder emitted diagnostics');
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
