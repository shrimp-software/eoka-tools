import assert from 'node:assert/strict';
import test from 'node:test';
import { assertRecordingQualification, recordingSchedule } from '../scripts/recording-qualification.mjs';

const build = '155.0.8059.39';
const rejected = evidence => assert.throws(() => assertRecordingQualification(evidence, build));
function completeEvidence() {
  return {
    status: 'passed',
    readiness: { chromeBuildId: build },
    checks: {
      chrome: { buildId: build },
      nativeRecordings: { artifactKind: 'finalized SDK MP4', browserVersion: `Chrome/${build}`, schedule: structuredClone(recordingSchedule), attemptsPerCase: 1 },
    },
    recordings: recordingSchedule.map(scheduled => ({
      ...scheduled,
      status: 'passed',
      record: { state: 'completed' },
      artifact: { container: 'mp4', mimeType: 'video/mp4' },
      errors: [],
      probeStderr: '',
      decodeStderr: '',
      advisories: [],
      sampleCount: 3,
      packetDts: [0, 1, 3000],
    })),
  };
}

test('a passing summary cannot qualify a different or unidentified running Chrome', () => {
  const evidence = completeEvidence();
  assertRecordingQualification(evidence, build);
  evidence.checks.nativeRecordings.browserVersion = 'Chrome/156.0.0.0';
  rejected(evidence);
  delete evidence.checks.nativeRecordings.browserVersion;
  rejected(evidence);
  evidence.checks.nativeRecordings.browserVersion = `HeadlessChrome/${build}`;
  assertRecordingQualification(evidence, build);
  evidence.readiness.chromeBuildId = '156.0.0.0';
  rejected(evidence);
});

test('qualification cannot omit, duplicate or retry required recording cases', () => {
  const missing = completeEvidence();
  missing.recordings.pop();
  rejected(missing);
  const duplicate = completeEvidence();
  duplicate.recordings[1] = structuredClone(duplicate.recordings[0]);
  rejected(duplicate);
  const narrowed = completeEvidence();
  narrowed.checks.nativeRecordings.schedule.pop();
  narrowed.recordings.pop();
  rejected(narrowed);
  const retried = completeEvidence();
  retried.checks.nativeRecordings.attemptsPerCase = 2;
  rejected(retried);
});

test('passed flags cannot hide decoder diagnostics or unpublished recordings', () => {
  const warned = completeEvidence();
  warned.recordings[0].decodeStderr = 'non monotonically increasing dts';
  rejected(warned);
  const unprobed = completeEvidence();
  delete unprobed.recordings[0].probeStderr;
  rejected(unprobed);
  const unpublished = completeEvidence();
  unpublished.recordings[0].record.state = 'failed';
  rejected(unpublished);
});

test('qualification checks actual timestamps rather than a reported monotonicity verdict', () => {
  const duplicate = completeEvidence();
  duplicate.recordings[0].nonmonotonicDts = [];
  duplicate.recordings[0].packetDts = [0, 0, 3000];
  rejected(duplicate);
  const backwards = completeEvidence();
  backwards.recordings[0].packetDts = [1, 0, 3000];
  rejected(backwards);
  const missing = completeEvidence();
  missing.recordings[0].packetDts = [];
  missing.recordings[0].sampleCount = 0;
  rejected(missing);
});

test('timestamp verification preserves exact integers above the JavaScript safe range', () => {
  const evidence = completeEvidence();
  evidence.recordings[0].packetDts = ['9007199254740992', '9007199254740993', '9007199254740994'];
  assertRecordingQualification(evidence, build);
  evidence.recordings[0].packetDts = evidence.recordings[0].packetDts.map(Number);
  rejected(evidence);
});

function vfrEvidence() {
  const evidence = completeEvidence();
  Object.assign(evidence.recordings[5], {
    sampleCount: 5,
    packetDts: [0, 1, 2, 3, 3972],
    packetDurations: [1, 1, 1, 3969, 999],
    decodeStderr: '[in#0/mov,mp4,m4a,3gp,3g2,mj2 @ 0x7fcfe3706340] DTS discontinuity in stream 0: packet 3 with DTS 3, packet 4 with DTS 3972\n',
    advisories: [{ source: 'decode', kind: 'vfr-rate-estimation', previousPacket: 3, nextPacket: 4 }],
  });
  return evidence;
}

test('continuous VFR estimation advisories are retained without changing media timing or diagnostics', () => {
  const evidence = vfrEvidence();
  const recording = evidence.recordings[5];
  recording.probeStderr = recording.decodeStderr.replace('in#0/', '');
  recording.advisories.unshift({ source: 'probe', kind: 'vfr-rate-estimation', previousPacket: 3, nextPacket: 4 });
  const original = structuredClone(evidence);
  assertRecordingQualification(evidence, build);
  assert.deepEqual(evidence, original);
});

test('a rate-estimation warning cannot excuse inconsistent, missing or inexact duration evidence', () => {
  for (const duration of [3968, null, Number.MAX_SAFE_INTEGER + 1]) {
    const evidence = vfrEvidence();
    evidence.recordings[5].packetDurations[3] = duration;
    rejected(evidence);
  }
});

test('diagnostic packet identities must match adjacent recorded timestamps', () => {
  for (const replacement of ['packet 2 with DTS 3', 'packet 99 with DTS 3', 'packet 3 with DTS 4']) {
    const evidence = vfrEvidence();
    evidence.recordings[5].decodeStderr = evidence.recordings[5].decodeStderr.replace('packet 3 with DTS 3', replacement);
    rejected(evidence);
  }
});

test('unknown decoder diagnostics remain fatal alongside a proven estimation advisory', () => {
  const evidence = vfrEvidence();
  evidence.recordings[5].decodeStderr += '[av1 @ 0x1234] Invalid temporal unit\n';
  rejected(evidence);
});

test('stored advisory classifications cannot replace or omit raw diagnostic verification', () => {
  const omitted = vfrEvidence();
  omitted.recordings[5].advisories = [];
  rejected(omitted);
  const unsupported = vfrEvidence();
  unsupported.recordings[5].decodeStderr = '';
  rejected(unsupported);
});

test('advisory verification compares exact integer timestamps rather than rounded numbers', () => {
  const evidence = vfrEvidence();
  const recording = evidence.recordings[5];
  const offset = 9007199254740992n;
  recording.packetDts = recording.packetDts.map(value => String(offset + BigInt(value)));
  recording.decodeStderr = `[in#0/mov,mp4,m4a,3gp,3g2,mj2 @ 0x1234] DTS discontinuity in stream 0: packet 3 with DTS ${offset + 3n}, packet 4 with DTS ${offset + 3972n}\n`;
  assertRecordingQualification(evidence, build);
  recording.packetDts = recording.packetDts.map(Number);
  rejected(evidence);
});
