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
}

fn demux(bytes: &[u8]) -> Demuxed {
    let mut output = Demuxed {
        dts: Vec::new(),
        durations: Vec::new(),
        payload: Vec::new(),
        duration_positions: Vec::new(),
    };
    for moof in descendants(bytes, b"moof") {
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
            assert_eq!(u32_at(bytes, run.body) & 0xffffff, 0x305);
            let count = u32_at(bytes, run.body + 4) as usize;
            let offset = u32_at(bytes, run.body + 8) as i32;
            let mut data = usize::try_from((moof.start as i64) + i64::from(offset)).unwrap();
            for index in 0..count {
                let position = run.body + 16 + index * 8;
                let duration = u32_at(bytes, position);
                let size = u32_at(bytes, position + 4) as usize;
                output.dts.push(dts);
                output.durations.push(duration);
                output.payload.push(bytes[data..data + size].to_vec());
                output.duration_positions.push(position);
                dts += u64::from(duration);
                data += size;
            }
        }
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
    let penultimate = before.duration_positions[before.duration_positions.len() - 2];
    let last = *before.duration_positions.last().unwrap();
    assert_eq!(u32_at(&output, penultimate), 1);
    assert_eq!(u32_at(&output, last), u32_at(bytes, last) - 1);
    for position in 0..bytes.len() {
        if !(penultimate..penultimate + 4).contains(&position)
            && !(last..last + 4).contains(&position)
        {
            assert_eq!(
                bytes[position], output[position],
                "unexpected mutation at {position}"
            );
        }
    }
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

#[tokio::test]
async fn multiple_nonterminal_and_terminal_zeros_are_rejected_without_writes() {
    let parsed = demux(REPAIRABLE_MP4);
    for values in [[0, 0, 999], [0, 499, 999], [499, 1, 0]] {
        let mut fixture = REPAIRABLE_MP4.to_vec();
        for (&position, value) in parsed.duration_positions.iter().zip(values) {
            put_u32(&mut fixture, position, value);
        }
        unchanged_error(&fixture).await;
    }
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
