use super::{repair, REPAIRABLE_MP4, VALID_MP4};
use std::io::{ErrorKind, SeekFrom};
use tokio::io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt};

const DEFECTIVE: [&[u8]; 4] = [
    REPAIRABLE_MP4,
    include_bytes!("../../../tests/fixtures/recordings/direct-20.mp4"),
    include_bytes!("../../../tests/fixtures/recordings/direct-44.mp4"),
    include_bytes!("../../../tests/fixtures/recordings/sdk-28.mp4"),
];

const CLEAN: [&[u8]; 5] = [
    include_bytes!("../../../tests/fixtures/recordings/clean-linux-x64-gnu.mp4"),
    include_bytes!("../../../tests/fixtures/recordings/clean-linux-arm64-gnu.mp4"),
    include_bytes!("../../../tests/fixtures/recordings/clean-darwin-x64.mp4"),
    include_bytes!("../../../tests/fixtures/recordings/clean-darwin-arm64.mp4"),
    include_bytes!("../../../tests/fixtures/recordings/clean-win32-x64.mp4"),
];

const ZERO_PREFIX: [&[u8]; 9] = [
    include_bytes!("../../../tests/fixtures/recordings/zero-prefix-linux-x64-gnu-120fps-67.mp4"),
    include_bytes!("../../../tests/fixtures/recordings/zero-prefix-darwin-x64-default-251.mp4"),
    include_bytes!("../../../tests/fixtures/recordings/zero-prefix-darwin-x64-120fps-67.mp4"),
    include_bytes!("../../../tests/fixtures/recordings/zero-prefix-darwin-arm64-default-167.mp4"),
    include_bytes!("../../../tests/fixtures/recordings/zero-prefix-darwin-arm64-default-101.mp4"),
    include_bytes!("../../../tests/fixtures/recordings/zero-prefix-darwin-arm64-120fps-67.mp4"),
    include_bytes!("../../../tests/fixtures/recordings/zero-prefix-darwin-arm64-120fps-251.mp4"),
    include_bytes!("../../../tests/fixtures/recordings/zero-prefix-darwin-arm64-native-0.mp4"),
    include_bytes!("../../../tests/fixtures/recordings/zero-prefix-darwin-arm64-native-2.mp4"),
];

#[derive(Clone, Copy)]
struct BoxRange {
    start: usize,
    body: usize,
    end: usize,
    kind: [u8; 4],
}

fn u32_at(bytes: &[u8], position: usize) -> u32 {
    u32::from_be_bytes(bytes[position..position + 4].try_into().unwrap())
}

fn u64_at(bytes: &[u8], position: usize) -> u64 {
    u64::from_be_bytes(bytes[position..position + 8].try_into().unwrap())
}

fn put_u32(bytes: &mut [u8], position: usize, value: u32) {
    bytes[position..position + 4].copy_from_slice(&value.to_be_bytes());
}

fn put_u64(bytes: &mut [u8], position: usize, value: u64) {
    bytes[position..position + 8].copy_from_slice(&value.to_be_bytes());
}

fn boxes(bytes: &[u8], start: usize, end: usize) -> Vec<BoxRange> {
    let mut result = Vec::new();
    let mut position = start;
    while position < end {
        let size = u32_at(bytes, position);
        let (size, header) = match size {
            0 => (end - position, 8),
            1 => (usize::try_from(u64_at(bytes, position + 8)).unwrap(), 16),
            size => (size as usize, 8),
        };
        assert!(size >= header && size <= end - position);
        result.push(BoxRange {
            start: position,
            body: position + header,
            end: position + size,
            kind: bytes[position + 4..position + 8].try_into().unwrap(),
        });
        position += size;
    }
    result
}

fn descendants(bytes: &[u8], kind: &[u8; 4]) -> Vec<BoxRange> {
    fn visit(bytes: &[u8], start: usize, end: usize, kind: &[u8; 4], output: &mut Vec<BoxRange>) {
        for header in boxes(bytes, start, end) {
            if &header.kind == kind {
                output.push(header);
            }
            if matches!(
                &header.kind,
                b"moov"
                    | b"trak"
                    | b"mdia"
                    | b"minf"
                    | b"stbl"
                    | b"mvex"
                    | b"moof"
                    | b"traf"
                    | b"mfra"
            ) {
                visit(bytes, header.body, header.end, kind, output);
            }
        }
    }
    let mut result = Vec::new();
    visit(bytes, 0, bytes.len(), kind, &mut result);
    result
}

struct Demuxed {
    dts: Vec<u64>,
    durations: Vec<u32>,
    payload: Vec<Vec<u8>>,
    duration_positions: Vec<usize>,
    fragments: Vec<std::ops::Range<usize>>,
}

fn demux(bytes: &[u8]) -> Demuxed {
    let mut output = Demuxed {
        dts: Vec::new(),
        durations: Vec::new(),
        payload: Vec::new(),
        duration_positions: Vec::new(),
        fragments: Vec::new(),
    };
    for moof in descendants(bytes, b"moof") {
        let first_sample = output.dts.len();
        let traf = boxes(bytes, moof.body, moof.end)
            .into_iter()
            .find(|header| header.kind == *b"traf")
            .unwrap();
        let children = boxes(bytes, traf.body, traf.end);
        let tfdt = children
            .iter()
            .find(|header| header.kind == *b"tfdt")
            .unwrap();
        let mut dts = if bytes[tfdt.body] == 1 {
            u64_at(bytes, tfdt.body + 4)
        } else {
            u64::from(u32_at(bytes, tfdt.body + 4))
        };
        for run in children.iter().filter(|header| header.kind == *b"trun") {
            let flags = u32_at(bytes, run.body) & 0xffffff;
            assert!(flags == 0x305 || flags == 0xb05);
            let stride = if flags & 0x800 == 0 { 8 } else { 12 };
            let count = u32_at(bytes, run.body + 4) as usize;
            let offset = u32_at(bytes, run.body + 8) as i32;
            let mut data = usize::try_from((moof.start as i64) + i64::from(offset)).unwrap();
            for index in 0..count {
                let position = run.body + 16 + index * stride;
                let duration = u32_at(bytes, position);
                let size = u32_at(bytes, position + 4) as usize;
                if flags & 0x800 != 0 {
                    assert_eq!(u32_at(bytes, position + 8), 0);
                }
                output.dts.push(dts);
                output.durations.push(duration);
                output.payload.push(bytes[data..data + size].to_vec());
                output.duration_positions.push(position);
                dts += u64::from(duration);
                data += size;
            }
        }
        output.fragments.push(first_sample..output.dts.len());
    }
    output
}

fn strict_dts(dts: &[u64]) -> bool {
    dts.windows(2).all(|pair| pair[0] < pair[1])
}

async fn apply(bytes: &[u8]) -> (std::io::Result<()>, Vec<u8>) {
    let directory = tempfile::tempdir().unwrap();
    let mut file = tokio::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create_new(true)
        .open(directory.path().join("recording.mp4"))
        .await
        .unwrap();
    file.write_all(bytes).await.unwrap();
    file.flush().await.unwrap();
    let result = repair(&mut file).await;
    file.seek(SeekFrom::Start(0)).await.unwrap();
    let mut output = Vec::new();
    file.read_to_end(&mut output).await.unwrap();
    (result, output)
}

async fn unchanged_error(bytes: &[u8]) {
    let (result, output) = apply(bytes).await;
    assert_eq!(result.unwrap_err().kind(), ErrorKind::InvalidData);
    assert_eq!(output, bytes);
}

async fn repaired_losslessly(bytes: &[u8]) {
    let before = demux(bytes);
    assert!(!strict_dts(&before.dts));
    let (result, output) = apply(bytes).await;
    result.unwrap();
    let after = demux(&output);
    assert!(strict_dts(&after.dts));
    assert_eq!(before.payload, after.payload);
    assert_eq!(
        before
            .durations
            .iter()
            .map(|&value| u64::from(value))
            .sum::<u64>(),
        after
            .durations
            .iter()
            .map(|&value| u64::from(value))
            .sum::<u64>()
    );
    assert_eq!(bytes.len(), output.len());
    assert_eq!(before.dts.len(), after.dts.len());
    assert_eq!(
        before.dts.last().unwrap() + u64::from(*before.durations.last().unwrap()),
        after.dts.last().unwrap() + u64::from(*after.durations.last().unwrap())
    );
    let mut expected = bytes.to_vec();
    for fragment in &before.fragments {
        let mut index = fragment.start;
        while index < fragment.end {
            if before.durations[index] != 0 {
                assert_eq!(before.dts[index], after.dts[index]);
                index += 1;
                continue;
            }
            let first_zero = index;
            while index < fragment.end && before.durations[index] == 0 {
                index += 1;
            }
            assert!(index < fragment.end);
            let zeros = u32::try_from(index - first_zero).unwrap();
            assert!(before.durations[index] > zeros);
            for zero in first_zero..index {
                put_u32(&mut expected, before.duration_positions[zero], 1);
                assert_eq!(
                    after.dts[zero],
                    before.dts[first_zero] + (zero - first_zero) as u64
                );
            }
            put_u32(
                &mut expected,
                before.duration_positions[index],
                before.durations[index] - zeros,
            );
            assert_eq!(after.dts[index], before.dts[index] + u64::from(zeros));
            assert_eq!(
                before.dts[index] + u64::from(before.durations[index]),
                after.dts[index] + u64::from(after.durations[index])
            );
            index += 1;
        }
    }
    assert_eq!(output, expected);
    let (result, repeated) = apply(&output).await;
    result.unwrap();
    assert_eq!(output, repeated);
}

#[tokio::test]
async fn retained_native_originals_receive_only_two_duration_writes() {
    for fixture in DEFECTIVE {
        repaired_losslessly(fixture).await;
    }
}

#[tokio::test]
async fn hosted_intermediate_and_consecutive_zero_originals_preserve_packets_and_endpoints() {
    for fixture in ZERO_PREFIX {
        repaired_losslessly(fixture).await;
    }
}

#[tokio::test]
async fn clean_native_recordings_from_all_five_platforms_remain_byte_identical() {
    for fixture in CLEAN.into_iter().chain([VALID_MP4]) {
        assert!(strict_dts(&demux(fixture).dts));
        let (result, output) = apply(fixture).await;
        result.unwrap();
        assert_eq!(output, fixture);
    }
}

#[tokio::test]
async fn terminal_zero_in_later_native_fragment_is_repaired() {
    let mut fixture = CLEAN[1].to_vec();
    let parsed = demux(&fixture);
    let position = parsed.duration_positions[parsed.duration_positions.len() - 2];
    put_u32(&mut fixture, position, 0);
    repaired_losslessly(&fixture).await;
}

#[tokio::test]
async fn sync_tail_without_an_index_for_that_sample_is_safe() {
    let mut fixture = REPAIRABLE_MP4.to_vec();
    let header = descendants(&fixture, b"tfhd")[0];
    put_u32(&mut fixture, header.body + 8, 0x02000000);
    repaired_losslessly(&fixture).await;
}

#[tokio::test]
async fn zero_sized_terminal_mdat_is_supported() {
    let mut fixture = REPAIRABLE_MP4.to_vec();
    let mdat = descendants(&fixture, b"mdat")[0];
    fixture.truncate(mdat.end);
    put_u32(&mut fixture, mdat.start, 0);
    repaired_losslessly(&fixture).await;
}

#[tokio::test]
async fn extended_file_type_and_media_box_sizes_are_supported() {
    for kind in [b"ftyp", b"mdat"] {
        let mut fixture = REPAIRABLE_MP4.to_vec();
        let header = descendants(&fixture, kind)[0];
        let size = header.end - header.start + 8;
        fixture.splice(header.body..header.body, (size as u64).to_be_bytes());
        put_u32(&mut fixture, header.start, 1);
        if kind == b"ftyp" {
            let tfra = descendants(&fixture, b"tfra")[0];
            let position = tfra.body + 24;
            let offset = u64_at(&fixture, position);
            put_u64(&mut fixture, position, offset + 8);
        } else {
            let trun = descendants(&fixture, b"trun")[0];
            let offset = u32_at(&fixture, trun.body + 8);
            put_u32(&mut fixture, trun.body + 8, offset + 8);
        }
        repaired_losslessly(&fixture).await;
    }
}

fn native_duration_runs(runs: &[&[u32]]) -> Vec<u8> {
    let packet = demux(REPAIRABLE_MP4).payload.remove(0);
    let moof = descendants(REPAIRABLE_MP4, b"moof")[0];
    let traf = descendants(REPAIRABLE_MP4, b"traf")[0];
    let template = descendants(REPAIRABLE_MP4, b"trun")[0];
    let mut output = REPAIRABLE_MP4[..template.start].to_vec();
    let mut offsets = Vec::new();
    let mut sample_count = 0;
    for durations in runs {
        assert!(!durations.is_empty());
        let start = output.len();
        output.extend_from_slice(&REPAIRABLE_MP4[template.start..template.body + 16]);
        put_u32(&mut output, start, 24 + durations.len() as u32 * 8);
        put_u32(&mut output, start + 12, durations.len() as u32);
        offsets.push((start + 16, sample_count * packet.len()));
        for &duration in *durations {
            output.extend_from_slice(&duration.to_be_bytes());
            output.extend_from_slice(&(packet.len() as u32).to_be_bytes());
        }
        sample_count += durations.len();
    }
    let fragment_end = output.len();
    put_u32(&mut output, moof.start, (fragment_end - moof.start) as u32);
    put_u32(&mut output, traf.start, (fragment_end - traf.start) as u32);
    for (position, payload_offset) in offsets {
        put_u32(
            &mut output,
            position,
            (fragment_end - moof.start + 8 + payload_offset) as u32,
        );
    }
    output.extend_from_slice(&(8 + sample_count as u32 * packet.len() as u32).to_be_bytes());
    output.extend_from_slice(b"mdat");
    for _ in 0..sample_count {
        output.extend_from_slice(&packet);
    }
    output
}

fn append_native_fragment(bytes: &mut Vec<u8>, durations: &[u32], origin: u64) {
    let mut fragment = native_duration_runs(&[durations]);
    let moof = descendants(&fragment, b"moof")[0];
    let tfdt = descendants(&fragment, b"tfdt")[0];
    let mfhd = descendants(&fragment, b"mfhd")[0];
    put_u64(&mut fragment, tfdt.body + 4, origin);
    put_u32(&mut fragment, mfhd.body + 4, 2);
    bytes.extend_from_slice(&fragment[moof.start..]);
}

fn implicit_duration_run(bytes: &mut Vec<u8>, run_index: usize, duration: u32) {
    let moof = descendants(bytes, b"moof")[0];
    let traf = descendants(bytes, b"traf")[0];
    let trex = descendants(bytes, b"trex")[0];
    let runs = descendants(bytes, b"trun");
    let run = runs[run_index];
    let count = u32_at(bytes, run.body + 4) as usize;
    let removed = count as u32 * 4;
    put_u32(bytes, trex.body + 12, duration);
    put_u32(bytes, moof.start, (moof.end - moof.start) as u32 - removed);
    put_u32(bytes, traf.start, (traf.end - traf.start) as u32 - removed);
    put_u32(bytes, run.start, (run.end - run.start) as u32 - removed);
    put_u32(bytes, run.body, 0x205);
    for header in &runs {
        let offset = u32_at(bytes, header.body + 8);
        put_u32(bytes, header.body + 8, offset - removed);
    }
    for index in (0..count).rev() {
        let position = run.body + 16 + index * 8;
        bytes.drain(position..position + 4);
    }
}

fn composition_offset_run(version: u32, composition: u32) -> Vec<u8> {
    let mut fixture = native_duration_runs(&[&[0, 0, 3, 17]]);
    let moof = descendants(&fixture, b"moof")[0];
    let traf = descendants(&fixture, b"traf")[0];
    let run = descendants(&fixture, b"trun")[0];
    let count = u32_at(&fixture, run.body + 4) as usize;
    let added = count as u32 * 4;
    put_u32(
        &mut fixture,
        moof.start,
        (moof.end - moof.start) as u32 + added,
    );
    put_u32(
        &mut fixture,
        traf.start,
        (traf.end - traf.start) as u32 + added,
    );
    put_u32(
        &mut fixture,
        run.start,
        (run.end - run.start) as u32 + added,
    );
    put_u32(&mut fixture, run.body, (version << 24) | 0xb05);
    let offset = u32_at(&fixture, run.body + 8);
    put_u32(&mut fixture, run.body + 8, offset + added);
    for index in (0..count).rev() {
        let position = run.body + 16 + (index + 1) * 8;
        let value = if index + 1 == count { composition } else { 0 };
        fixture.splice(position..position, value.to_be_bytes());
    }
    fixture
}

#[tokio::test]
async fn explicit_nonterminal_and_consecutive_zero_runs_are_repaired() {
    let parsed = demux(REPAIRABLE_MP4);
    for values in [[0, 0, 999], [0, 499, 999]] {
        let mut fixture = REPAIRABLE_MP4.to_vec();
        for (&position, value) in parsed.duration_positions.iter().zip(values) {
            put_u32(&mut fixture, position, value);
        }
        repaired_losslessly(&fixture).await;
    }
}

#[tokio::test]
async fn multiple_explicit_runs_and_runs_across_truns_preserve_later_timestamps() {
    for runs in [
        vec![&[0, 0, 3, 17, 0, 5, 19, 0, 0, 0, 4, 23][..]],
        vec![
            &[0][..],
            &[0, 3, 17, 0][..],
            &[5, 19, 0, 0][..],
            &[0, 4, 23][..],
        ],
        vec![&[17, 0, 0, 3, 0, u32::MAX, 23][..]],
    ] {
        repaired_losslessly(&native_duration_runs(&runs)).await;
    }
}

#[tokio::test]
async fn explicit_zero_runs_with_insufficient_room_or_no_donor_are_rejected_without_writes() {
    for durations in [
        &[0, 1, 999][..],
        &[0, 0, 1, 999][..],
        &[0, 0, 2, 999][..],
        &[0, 0, 0, 3, 999][..],
        &[499, 1, 0][..],
        &[499, 0, 0][..],
        &[0, 0, 0][..],
        &[0, 3, 17, 0, 0, 2][..],
    ] {
        unchanged_error(&native_duration_runs(&[durations])).await;
    }
}

#[tokio::test]
async fn explicit_zero_run_crossing_fragment_boundary_is_rejected_before_writes() {
    let mut fixture = ZERO_PREFIX[8].to_vec();
    let parsed = demux(&fixture);
    let boundary = parsed.fragments[0].end - 1;
    put_u32(&mut fixture, parsed.duration_positions[boundary], 0);
    unchanged_error(&fixture).await;
}

#[tokio::test]
async fn correctable_early_run_does_not_mutate_before_late_metadata_is_validated() {
    let original = ZERO_PREFIX[8];
    let last_run = *descendants(original, b"trun").last().unwrap();
    let last_origin = *descendants(original, b"tfdt").last().unwrap();
    for (position, value) in [(last_run.body + 4, u32::MAX), (last_run.body + 8, 0)] {
        let mut fixture = original.to_vec();
        put_u32(&mut fixture, position, value);
        unchanged_error(&fixture).await;
    }
    let mut fixture = REPAIRABLE_MP4.to_vec();
    let parsed = demux(&fixture);
    put_u32(&mut fixture, parsed.duration_positions[0], 0);
    put_u32(&mut fixture, parsed.duration_positions[1], 499);
    let index = descendants(&fixture, b"tfra")[0];
    put_u32(&mut fixture, index.body + 8, u32::MAX);
    unchanged_error(&fixture).await;
    let mut fixture = original.to_vec();
    put_u64(&mut fixture, last_origin.body + 4, 0);
    unchanged_error(&fixture).await;
    let mut fixture = original.to_vec();
    fixture.extend_from_slice(&[0, 0, 0, 8, b's', b'i', b'd', b'x']);
    unchanged_error(&fixture).await;
    let mut fixture = original.to_vec();
    let parsed = demux(&fixture);
    let count = parsed.duration_positions.len();
    put_u32(&mut fixture, parsed.duration_positions[count - 2], 0);
    put_u32(&mut fixture, parsed.duration_positions[count - 1], 1);
    unchanged_error(&fixture).await;
}

#[tokio::test]
async fn corrected_fragment_tail_cannot_collide_with_the_next_decode_origin() {
    for origin in [1, 2] {
        let mut fixture = native_duration_runs(&[&[0, 0, 999]]);
        append_native_fragment(&mut fixture, &[999], origin);
        unchanged_error(&fixture).await;
    }
    let mut safe_overlap = native_duration_runs(&[&[0, 0, 999]]);
    append_native_fragment(&mut safe_overlap, &[999], 3);
    repaired_losslessly(&safe_overlap).await;
}

#[tokio::test]
async fn zero_run_and_duplicate_terminal_fragment_cannot_produce_a_decode_collision() {
    for durations in [&[0, 999][..], &[0, 0, 999][..], &[499, 0, 999][..]] {
        let mut fixture = native_duration_runs(&[durations]);
        let original_tail = *demux(&fixture).dts.last().unwrap();
        append_native_fragment(&mut fixture, &[999], original_tail);
        unchanged_error(&fixture).await;
    }
}

#[tokio::test]
async fn independent_zero_run_and_terminal_fragment_repairs_preserve_the_presentation_end() {
    let mut fixture = native_duration_runs(&[&[0, 3, 499, 999]]);
    append_native_fragment(&mut fixture, &[999], 502);
    let before = demux(&fixture);
    let (result, output) = apply(&fixture).await;
    result.unwrap();
    let after = demux(&output);
    assert!(strict_dts(&after.dts));
    assert_eq!(after.dts, [0, 1, 3, 502, 503]);
    assert_eq!(after.durations, [1, 2, 499, 999, 998]);
    assert_eq!(before.payload, after.payload);
    assert_eq!(before.dts.len(), after.dts.len());
    assert_eq!(fixture.len(), output.len());
    assert_eq!(
        before.dts.last().unwrap() + u64::from(*before.durations.last().unwrap()),
        after.dts.last().unwrap() + u64::from(*after.durations.last().unwrap())
    );
    let origin = *descendants(&fixture, b"tfdt").last().unwrap();
    let mut expected = fixture.clone();
    put_u32(&mut expected, before.duration_positions[0], 1);
    put_u32(&mut expected, before.duration_positions[1], 2);
    put_u32(
        &mut expected,
        *before.duration_positions.last().unwrap(),
        998,
    );
    put_u64(&mut expected, origin.body + 4, 503);
    assert_eq!(output, expected);
    let (result, repeated) = apply(&output).await;
    result.unwrap();
    assert_eq!(output, repeated);
}

#[tokio::test]
async fn implicit_zero_and_implicit_donor_runs_are_rejected_before_any_writes() {
    let mut implicit_zero = native_duration_runs(&[&[0, 3, 19], &[0, 0]]);
    implicit_duration_run(&mut implicit_zero, 1, 0);
    unchanged_error(&implicit_zero).await;
    let mut implicit_donor = native_duration_runs(&[&[0, 0], &[5, 19]]);
    implicit_duration_run(&mut implicit_donor, 1, 5);
    unchanged_error(&implicit_donor).await;
}

#[tokio::test]
async fn zero_run_repair_requires_zero_composition_offsets_and_av1() {
    for version in [0, 1] {
        repaired_losslessly(&composition_offset_run(version, 0)).await;
        unchanged_error(&composition_offset_run(version, 1)).await;
    }
    unchanged_error(&composition_offset_run(1, u32::MAX)).await;
    let mut unsupported_codec = native_duration_runs(&[&[0, 0, 3, 17]]);
    unsupported_codec[534..538].copy_from_slice(b"vp09");
    unchanged_error(&unsupported_codec).await;
}

#[tokio::test]
async fn insufficient_terminal_duration_is_rejected_but_integer_limits_are_safe() {
    for final_duration in [0, 1, 2, u32::MAX] {
        let mut fixture = REPAIRABLE_MP4.to_vec();
        put_u32(&mut fixture, 850, final_duration);
        if final_duration < 2 {
            unchanged_error(&fixture).await;
        } else {
            repaired_losslessly(&fixture).await;
        }
    }
}

#[tokio::test]
async fn malformed_sizes_truncation_and_integer_overflow_are_rejected_without_writes() {
    for length in [0, 1, 7, 35, 737, 809, 857, 1000, 1592, 1668] {
        unchanged_error(&REPAIRABLE_MP4[..length]).await;
    }
    for size in [1, 2, 7, u32::MAX] {
        let mut fixture = REPAIRABLE_MP4.to_vec();
        put_u32(&mut fixture, 810, size);
        unchanged_error(&fixture).await;
    }
    let mut fixture = REPAIRABLE_MP4.to_vec();
    put_u32(&mut fixture, 0, 1);
    put_u64(&mut fixture, 8, u64::MAX);
    unchanged_error(&fixture).await;
    let mut fixture = REPAIRABLE_MP4.to_vec();
    put_u64(&mut fixture, 802, u64::MAX - 1000);
    unchanged_error(&fixture).await;
}

#[tokio::test]
async fn unsafe_offsets_counts_defaults_and_conflicting_tracks_are_rejected_without_writes() {
    for (position, value) in [
        (826, 0),
        (826, u32::MAX),
        (826, 129),
        (822, u32::MAX),
        (822, 0),
        (778, 2),
        (718, 2),
        (312, 0),
        (814, 0x01000b05),
        (774, 0x00030020),
        (774, 0x00000020),
    ] {
        let mut fixture = REPAIRABLE_MP4.to_vec();
        put_u32(&mut fixture, position, value);
        unchanged_error(&fixture).await;
    }
    let mut fixture = REPAIRABLE_MP4.to_vec();
    let mdat = descendants(&fixture, b"mdat")[0];
    put_u32(&mut fixture, mdat.start, (mdat.end - mdat.start - 1) as u32);
    unchanged_error(&fixture).await;
}

#[tokio::test]
async fn unsupported_codec_and_timing_metadata_only_block_defective_files() {
    for original in [REPAIRABLE_MP4, VALID_MP4] {
        let mut fixture = original.to_vec();
        fixture[534..538].copy_from_slice(b"vp09");
        let (result, output) = apply(&fixture).await;
        if original == REPAIRABLE_MP4 {
            assert_eq!(result.unwrap_err().kind(), ErrorKind::InvalidData);
        } else {
            result.unwrap();
        }
        assert_eq!(output, fixture);
    }
    let mut fixture = REPAIRABLE_MP4.to_vec();
    fixture.extend_from_slice(&[0, 0, 0, 8, b's', b'i', b'd', b'x']);
    unchanged_error(&fixture).await;
}

#[tokio::test]
async fn changed_sample_index_and_inconsistent_index_times_are_rejected_without_writes() {
    let tfra = descendants(REPAIRABLE_MP4, b"tfra")[0];
    for (relative, value) in [(16, 1u64), (24, 739)] {
        let mut fixture = REPAIRABLE_MP4.to_vec();
        put_u64(&mut fixture, tfra.body + relative, value);
        unchanged_error(&fixture).await;
    }
    let mut fixture = REPAIRABLE_MP4.to_vec();
    put_u32(&mut fixture, tfra.body + 40, 3);
    unchanged_error(&fixture).await;
}

#[tokio::test]
async fn decreasing_fragment_timestamps_are_not_normalized() {
    let mut fixture = CLEAN[1].to_vec();
    let origins = descendants(&fixture, b"tfdt");
    put_u64(&mut fixture, origins[1].body + 4, 0);
    unchanged_error(&fixture).await;
}

fn split_keyframe_tail(indexed: bool, first_fragment_single_sample: bool) -> Vec<u8> {
    let mut packets = demux(REPAIRABLE_MP4);
    packets.payload[2] = packets.payload[0].clone();
    let first_indices: &[usize] = if first_fragment_single_sample {
        &[0]
    } else {
        &[0, 1]
    };
    let tail_time = if first_fragment_single_sample { 0 } else { 499 };
    let mut output = REPAIRABLE_MP4[..738].to_vec();
    let mut offsets = Vec::new();
    for (sequence, indices, time) in [(1, first_indices, 0), (2, &[2usize][..], tail_time)] {
        offsets.push(output.len() as u64);
        let mut fragment = REPAIRABLE_MP4[738..834].to_vec();
        for &index in indices {
            let duration = if sequence == 1 && index + 1 == first_indices.len() {
                999
            } else {
                packets.durations[index]
            };
            fragment.extend_from_slice(&duration.to_be_bytes());
            fragment.extend_from_slice(&(packets.payload[index].len() as u32).to_be_bytes());
        }
        let length = fragment.len() as u32;
        put_u32(&mut fragment, 0, length);
        put_u32(&mut fragment, 20, sequence);
        put_u32(&mut fragment, 24, length - 24);
        put_u64(&mut fragment, 64, time);
        put_u32(&mut fragment, 72, length - 72);
        put_u32(&mut fragment, 84, indices.len() as u32);
        put_u32(&mut fragment, 88, length + 8);
        output.extend_from_slice(&fragment);
        let payload_size = indices
            .iter()
            .map(|&index| packets.payload[index].len())
            .sum::<usize>();
        output.extend_from_slice(&(payload_size as u32 + 8).to_be_bytes());
        output.extend_from_slice(b"mdat");
        for &index in indices {
            output.extend_from_slice(&packets.payload[index]);
        }
    }
    if indexed {
        let mut index = REPAIRABLE_MP4[1593..1669].to_vec();
        index.splice(60..60, [0u8; 28]);
        put_u32(&mut index, 0, 104);
        put_u32(&mut index, 8, 80);
        put_u32(&mut index, 28, 2);
        put_u64(&mut index, 40, offsets[0]);
        put_u64(&mut index, 60, tail_time);
        put_u64(&mut index, 68, offsets[1]);
        for position in [76, 80, 84] {
            put_u32(&mut index, position, 1);
        }
        put_u32(&mut index, 100, 104);
        output.extend_from_slice(&index);
    }
    output
}

#[tokio::test]
async fn terminal_single_sample_keyframe_fragment_repairs_decode_origin_duration_and_index() {
    for indexed in [false, true] {
        for first_fragment_single_sample in [false, true] {
            let fixture = split_keyframe_tail(indexed, first_fragment_single_sample);
            let before = demux(&fixture);
            assert!(!strict_dts(&before.dts));
            let (result, output) = apply(&fixture).await;
            result.unwrap();
            let after = demux(&output);
            assert!(strict_dts(&after.dts));
            assert_eq!(before.payload, after.payload);
            assert_eq!(fixture.len(), output.len());
            assert_eq!(
                before.dts.last().unwrap() + u64::from(*before.durations.last().unwrap()),
                after.dts.last().unwrap() + u64::from(*after.durations.last().unwrap())
            );
            let origin = *descendants(&fixture, b"tfdt").last().unwrap();
            let duration = *before.duration_positions.last().unwrap();
            let mut expected = fixture.clone();
            put_u64(
                &mut expected,
                origin.body + 4,
                before.dts.last().unwrap() + 1,
            );
            put_u32(
                &mut expected,
                duration,
                before.durations.last().unwrap() - 1,
            );
            if indexed {
                let tfra = descendants(&fixture, b"tfra")[0];
                put_u64(
                    &mut expected,
                    tfra.body + 44,
                    before.dts.last().unwrap() + 1,
                );
            }
            assert_eq!(output, expected);
            let (result, repeated) = apply(&output).await;
            result.unwrap();
            assert_eq!(repeated, output);
        }
    }
}

#[tokio::test]
async fn malformed_cross_fragment_tail_is_rejected_before_any_mutation() {
    for indexed in [false, true] {
        let fixture = split_keyframe_tail(indexed, false);
        let parsed = demux(&fixture);
        let duration = *parsed.duration_positions.last().unwrap();
        for value in [0, 1] {
            let mut defective = fixture.clone();
            put_u32(&mut defective, duration, value);
            unchanged_error(&defective).await;
        }
        let mut defective = fixture.clone();
        let run = *descendants(&fixture, b"trun").last().unwrap();
        put_u32(&mut defective, run.body + 12, 0x01010000);
        unchanged_error(&defective).await;
    }
}

#[tokio::test]
async fn duplicate_fragment_boundary_followed_by_another_fragment_is_rejected() {
    let mut fixture = split_keyframe_tail(false, false);
    let tail = *descendants(&fixture, b"moof").last().unwrap();
    let mut following = fixture[tail.start..].to_vec();
    let sequence = descendants(&following, b"mfhd")[0];
    put_u32(&mut following, sequence.body + 4, 3);
    let origin = descendants(&following, b"tfdt")[0];
    put_u64(&mut following, origin.body + 4, 1498);
    fixture.extend_from_slice(&following);
    unchanged_error(&fixture).await;
}

#[tokio::test]
async fn narrow_decode_origin_overflow_is_rejected_without_partial_duration_writes() {
    for timestamp in [u32::MAX - 1, u32::MAX] {
        let mut fixture = split_keyframe_tail(false, true);
        let origins = descendants(&fixture, b"tfdt");
        put_u64(&mut fixture, origins[0].body + 4, u64::from(timestamp));
        put_u64(&mut fixture, origins[1].body + 4, u64::from(timestamp));
        let final_moof = *descendants(&fixture, b"moof").last().unwrap();
        let final_traf = *descendants(&fixture, b"traf").last().unwrap();
        let final_run = *descendants(&fixture, b"trun").last().unwrap();
        put_u32(
            &mut fixture,
            final_moof.start,
            (final_moof.end - final_moof.start - 4) as u32,
        );
        put_u32(
            &mut fixture,
            final_traf.start,
            (final_traf.end - final_traf.start - 4) as u32,
        );
        put_u32(
            &mut fixture,
            final_run.body + 8,
            (final_moof.end - final_moof.start + 4) as u32,
        );
        put_u32(&mut fixture, origins[1].start, 16);
        put_u32(&mut fixture, origins[1].body, 0);
        fixture.drain(origins[1].body + 4..origins[1].body + 8);
        if timestamp == u32::MAX {
            unchanged_error(&fixture).await;
        } else {
            let (result, output) = apply(&fixture).await;
            result.unwrap();
            let parsed = demux(&output);
            assert!(strict_dts(&parsed.dts));
            assert_eq!(*parsed.dts.last().unwrap(), u64::from(u32::MAX));
        }
    }
}

#[tokio::test]
async fn default_only_runs_validate_aggregate_ranges_and_timestamps() {
    let payload = demux(VALID_MP4).payload.remove(0);
    let moof = descendants(VALID_MP4, b"moof")[0];
    let traf = descendants(VALID_MP4, b"traf")[0];
    let tfhd = descendants(VALID_MP4, b"tfhd")[0];
    let tfdt = descendants(VALID_MP4, b"tfdt")[0];
    let trex = descendants(VALID_MP4, b"trex")[0];
    let run = descendants(VALID_MP4, b"trun")[0];
    let mut fixture = VALID_MP4[..run.start].to_vec();
    fixture.extend_from_slice(&20u32.to_be_bytes());
    fixture.extend_from_slice(b"trun");
    fixture.extend_from_slice(&1u32.to_be_bytes());
    fixture.extend_from_slice(&3u32.to_be_bytes());
    fixture.extend_from_slice(&0u32.to_be_bytes());
    let fragment_end = fixture.len();
    put_u32(&mut fixture, moof.start, (fragment_end - moof.start) as u32);
    put_u32(&mut fixture, traf.start, (fragment_end - traf.start) as u32);
    put_u32(
        &mut fixture,
        run.body + 8,
        (fragment_end - moof.start + 8) as u32,
    );
    put_u32(&mut fixture, trex.body + 12, 400);
    put_u32(&mut fixture, trex.body + 16, payload.len() as u32);
    put_u32(&mut fixture, tfhd.end - 4, 0x02000000);
    fixture.extend_from_slice(&(8 + 3 * payload.len() as u32).to_be_bytes());
    fixture.extend_from_slice(b"mdat");
    for _ in 0..3 {
        fixture.extend_from_slice(&payload);
    }
    let (result, output) = apply(&fixture).await;
    result.unwrap();
    assert_eq!(output, fixture);

    let mut excessive_count = fixture.clone();
    put_u32(&mut excessive_count, run.body + 4, u32::MAX);
    unchanged_error(&excessive_count).await;

    let mut zero_duration = fixture.clone();
    put_u32(&mut zero_duration, trex.body + 12, 0);
    unchanged_error(&zero_duration).await;

    put_u64(&mut fixture, tfdt.body + 4, u64::MAX - 1200);
    let (result, output) = apply(&fixture).await;
    result.unwrap();
    assert_eq!(output, fixture);
    put_u64(&mut fixture, tfdt.body + 4, u64::MAX - 800);
    unchanged_error(&fixture).await;
}
