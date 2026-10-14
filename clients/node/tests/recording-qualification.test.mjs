import assert from 'node:assert/strict';
import test from 'node:test';
import { assertRecordingQualification, recordingSchedule } from '../scripts/recording-qualification.mjs';

const build = '155.0.8059.39';
const rejected = evidence => assert.throws(() => assertRecordingQualification(evidence, build), { code: 'ERR_ASSERTION' });
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
