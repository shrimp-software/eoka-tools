use std::io::{self, ErrorKind, SeekFrom};

use tokio::fs::File;
use tokio::io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt};

type Result<T, E = io::Error> = std::result::Result<T, E>;

fn invalid(message: &'static str) -> io::Error {
    io::Error::new(ErrorKind::InvalidData, message)
}

fn require(condition: bool, message: &'static str) -> Result<()> {
    if condition {
        Ok(())
    } else {
        Err(invalid(message))
    }
}

fn add(a: u64, b: u64) -> Result<u64> {
    a.checked_add(b)
        .ok_or_else(|| invalid("MP4 integer overflow"))
}

#[derive(Clone, Copy)]
struct BoxHeader {
    start: u64,
    body: u64,
    end: u64,
    kind: [u8; 4],
}

struct Reader<'a> {
    file: &'a mut File,
    length: u64,
    cache: [u8; 8192],
    cache_start: u64,
    cache_end: u64,
}

impl Reader<'_> {
    async fn bytes<const N: usize>(&mut self, position: u64, end: u64) -> Result<[u8; N]> {
        let stop = add(position, N as u64)?;
        require(stop <= end && end <= self.length, "Truncated MP4 metadata")?;
        if position < self.cache_start || stop > self.cache_end {
            self.file.seek(SeekFrom::Start(position)).await?;
            let count = (self.length - position).min(self.cache.len() as u64) as usize;
            self.file.read_exact(&mut self.cache[..count]).await?;
            self.cache_start = position;
            self.cache_end = position + count as u64;
        }
        let offset = (position - self.cache_start) as usize;
        let mut value = [0; N];
        value.copy_from_slice(&self.cache[offset..offset + N]);
        Ok(value)
    }

    async fn u32(&mut self, position: u64, end: u64) -> Result<u32> {
        Ok(u32::from_be_bytes(self.bytes(position, end).await?))
    }

    async fn u64(&mut self, position: u64, end: u64) -> Result<u64> {
        Ok(u64::from_be_bytes(self.bytes(position, end).await?))
    }

    async fn header(&mut self, position: u64, end: u64) -> Result<BoxHeader> {
        let bytes = self.bytes::<8>(position, end).await?;
        let short = u32::from_be_bytes(bytes[..4].try_into().unwrap());
        let kind = bytes[4..].try_into().unwrap();
        let (size, header_size) = match short {
            0 => (end - position, 8),
            1 => (self.u64(add(position, 8)?, end).await?, 16),
            value => (u64::from(value), 8),
        };
        require(size >= header_size, "Invalid MP4 box size")?;
        let box_end = add(position, size)?;
        require(box_end <= end, "Truncated MP4 box")?;
        Ok(BoxHeader {
            start: position,
            body: add(position, header_size)?,
            end: box_end,
            kind,
        })
    }
}

#[derive(Default)]
struct Track {
    id: Option<u32>,
    timescale: Option<u32>,
    video: bool,
    av1: bool,
    trex: Option<[u32; 5]>,
    supported: bool,
}

async fn inspect_metadata(reader: &mut Reader<'_>, root: BoxHeader) -> Result<Track> {
    let mut track = Track {
        supported: true,
        ..Track::default()
    };
    let mut stack = [(0u64, 0u64, [0u8; 4]); 8];
    stack[0] = (root.body, root.end, root.kind);
    let mut depth = 1;
    let mut tracks = 0;
    let mut descriptions = 0;
    let mut handlers = 0;
    let mut required_boxes = 0u16;
    while depth > 0 {
        let (position, end, parent) = stack[depth - 1];
        if position == end {
            depth -= 1;
            continue;
        }
        let header = reader.header(position, end).await?;
        stack[depth - 1].0 = header.end;
        let p = header.body;
        let expected_parent = match &header.kind {
            b"mvhd" | b"trak" | b"mvex" => Some(*b"moov"),
            b"tkhd" | b"mdia" => Some(*b"trak"),
            b"mdhd" | b"hdlr" | b"minf" => Some(*b"mdia"),
            b"vmhd" | b"dinf" | b"stbl" => Some(*b"minf"),
            b"stsd" | b"stts" | b"stsc" | b"stsz" | b"stco" | b"co64" => Some(*b"stbl"),
            b"trex" => Some(*b"mvex"),
            b"dref" => Some(*b"dinf"),
            _ => None,
        };
        require(
            expected_parent.is_none_or(|expected| expected == parent),
            "Misplaced MP4 metadata",
        )?;
        let unique_bit = match &header.kind {
            b"mvhd" => 1,
            b"mdia" => 2,
            b"minf" => 4,
            b"stbl" => 8,
            b"mvex" => 16,
            b"dinf" => 32,
            b"vmhd" => 64,
            b"dref" => 128,
            b"stts" => 256,
            b"stsc" => 512,
            b"stsz" => 1024,
            b"stco" | b"co64" => 2048,
            _ => 0,
        };
        require(
            required_boxes & unique_bit == 0,
            "Duplicate MP4 metadata box",
        )?;
        required_boxes |= unique_bit;
        match &header.kind {
            b"trak" | b"mdia" | b"minf" | b"stbl" | b"mvex" | b"dinf" => {
                if header.kind == *b"trak" {
                    tracks += 1;
                    require(tracks == 1, "Multiple MP4 tracks are unsupported")?;
                }
                require(depth < stack.len(), "MP4 nesting limit exceeded")?;
                stack[depth] = (p, header.end, header.kind);
                depth += 1;
            }
            b"tkhd" | b"mdhd" => {
                let full = reader.u32(p, header.end).await?;
                let version = full >> 24;
                require(version <= 1, "Unsupported MP4 track header version")?;
                let field = add(p, if version == 1 { 20 } else { 12 })?;
                let value = reader.u32(field, header.end).await?;
                require(value != 0, "Invalid MP4 track identity or timescale")?;
                let slot = if header.kind == *b"tkhd" {
                    &mut track.id
                } else {
                    &mut track.timescale
                };
                require(slot.replace(value).is_none(), "Duplicate MP4 track header")?;
                let expected = if header.kind == *b"tkhd" {
                    if version == 1 {
                        96
                    } else {
                        84
                    }
                } else if version == 1 {
                    36
                } else {
                    24
                };
                require(
                    header.end - p == expected,
                    "Invalid MP4 track header length",
                )?;
            }
            b"hdlr" => {
                require(header.end - p >= 24, "Truncated MP4 handler")?;
                require(
                    reader.u32(p, header.end).await? == 0,
                    "Unsupported MP4 handler",
                )?;
                handlers += 1;
                track.video = reader.bytes::<4>(p + 8, header.end).await? == *b"vide";
            }
            b"trex" => {
                require(header.end - p == 24, "Invalid MP4 sample defaults")?;
                require(
                    reader.u32(p, header.end).await? == 0,
                    "Unsupported MP4 sample defaults",
                )?;
                let mut values = [0; 5];
                for (index, value) in values.iter_mut().enumerate() {
                    *value = reader.u32(p + 4 + index as u64 * 4, header.end).await?;
                }
                require(
                    track.trex.replace(values).is_none(),
                    "Duplicate MP4 sample defaults",
                )?;
            }
            b"stsd" => {
                require(
                    reader.u32(p, header.end).await? == 0,
                    "Unsupported MP4 sample description",
                )?;
                require(
                    reader.u32(p + 4, header.end).await? == 1,
                    "Multiple MP4 sample descriptions",
                )?;
                descriptions += 1;
                let sample = reader.header(p + 8, header.end).await?;
                require(
                    sample.end == header.end,
                    "Invalid MP4 sample description length",
                )?;
                track.av1 = sample.kind == *b"av01";
                if track.av1 {
                    require(sample.end - sample.body >= 78, "Truncated AV1 sample entry")?;
                    let reference = reader.bytes::<2>(sample.body + 6, sample.end).await?;
                    require(reference == [0, 1], "External MP4 sample data")?;
                    let mut child = sample.body + 78;
                    let mut configurations = 0;
                    while child < sample.end {
                        let nested = reader.header(child, sample.end).await?;
                        match &nested.kind {
                            b"av1C" => {
                                configurations += 1;
                                require(
                                    nested.end - nested.body >= 4,
                                    "Truncated AV1 configuration",
                                )?;
                                require(
                                    reader.bytes::<1>(nested.body, nested.end).await? == [0x81],
                                    "Unsupported AV1 configuration",
                                )?;
                            }
                            b"pasp" | b"btrt" | b"colr" => {}
                            _ => track.supported = false,
                        }
                        child = nested.end;
                    }
                    require(
                        configurations == 1,
                        "Missing or duplicate AV1 configuration",
                    )?;
                }
            }
            b"stts" | b"stsc" | b"stco" | b"co64" => {
                require(header.end - p == 8, "Nonempty classic MP4 sample table")?;
                require(
                    reader.u64(p, header.end).await? == 0,
                    "Nonempty classic MP4 sample table",
                )?;
            }
            b"stsz" => {
                require(header.end - p == 12, "Invalid MP4 sample size table")?;
                require(
                    reader.u64(p, header.end).await? == 0
                        && reader.u32(p + 8, header.end).await? == 0,
                    "Nonempty classic MP4 sample table",
                )?;
            }
            b"dref" => {
                require(
                    reader.u32(p, header.end).await? == 0
                        && reader.u32(p + 4, header.end).await? == 1,
                    "Unsupported MP4 data references",
                )?;
                let reference = reader.header(p + 8, header.end).await?;
                let reference_length = reference.end - reference.body;
                require(
                    reference.kind == *b"url "
                        && reference.end == header.end
                        && (reference_length == 4 || reference_length == 5)
                        && reader.u32(reference.body, reference.end).await? == 1,
                    "External MP4 data references",
                )?;
                if reference_length == 5 {
                    require(
                        reader.bytes::<1>(reference.body + 4, reference.end).await? == [0],
                        "External MP4 data reference location",
                    )?;
                }
            }
            b"mvhd" => {
                let version = reader.u32(p, header.end).await? >> 24;
                require(
                    version <= 1 && header.end - p == if version == 1 { 112 } else { 100 },
                    "Invalid MP4 movie header",
                )?;
            }
            b"vmhd" => require(header.end - p == 12, "Invalid MP4 video header")?,
            b"free" | b"skip" => {}
            _ => track.supported = false,
        }
    }
    require(
        tracks == 1
            && descriptions == 1
            && handlers == 1
            && track.id.is_some()
            && track.timescale.is_some(),
        "Incomplete MP4 video track",
    )?;
    require(required_boxes == 4095, "Incomplete MP4 movie structure")?;
    let defaults = track
        .trex
        .ok_or_else(|| invalid("Missing MP4 fragment defaults"))?;
    require(
        Some(defaults[0]) == track.id && defaults[1] == 1,
        "Conflicting MP4 fragment defaults",
    )?;
    Ok(track)
}

#[derive(Clone, Copy)]
struct TimeField {
    position: u64,
    wide: bool,
    value: u64,
}

impl TimeField {
    fn incremented(self) -> Result<Self> {
        let value = add(self.value, 1)?;
        require(
            self.wide || value <= u64::from(u32::MAX),
            "MP4 timestamp field overflow",
        )?;
        Ok(Self { value, ..self })
    }

    async fn write(self, file: &mut File) -> Result<()> {
        file.seek(SeekFrom::Start(self.position)).await?;
        if self.wide {
            file.write_all(&self.value.to_be_bytes()).await?;
        } else {
            file.write_all(&(self.value as u32).to_be_bytes()).await?;
        }
        Ok(())
    }
}

#[derive(Clone, Copy)]
struct Sample {
    dts: u64,
    duration: u32,
    duration_position: Option<u64>,
    fragment: u64,
    ordinal: u64,
    composition: i64,
    origin: Option<TimeField>,
    sync: bool,
}

#[derive(Default)]
struct Timeline {
    last: Option<Sample>,
    zero: Option<Sample>,
    boundary: Option<Sample>,
    boundary_index: Option<TimeField>,
    samples: u64,
    supported: bool,
    sequence: Option<u32>,
}

impl Timeline {
    fn sample(&mut self, mut sample: Sample) -> Result<()> {
        self.samples = add(self.samples, 1)?;
        sample.ordinal = self.samples;
        add(sample.dts, u64::from(sample.duration))?;
        if sample.composition >= 0 {
            add(sample.dts, sample.composition as u64)?;
        } else {
            require(
                sample.dts >= sample.composition.unsigned_abs(),
                "Negative MP4 presentation timestamp",
            )?;
        }
        if sample.composition != 0 {
            self.supported = false;
        }
        if let Some(previous) = self.last {
            if sample.dts == previous.dts && sample.fragment != previous.fragment {
                require(
                    previous.duration != 0
                        && sample.origin.is_some()
                        && sample.sync
                        && self.boundary.replace(sample).is_none(),
                    "Unsupported duplicate MP4 fragment timestamp",
                )?;
            } else {
                require(
                    sample.dts > previous.dts
                        || (sample.dts == previous.dts
                            && previous.duration == 0
                            && sample.fragment == previous.fragment),
                    "Non-increasing MP4 decode timestamps",
                )?;
            }
        }
        if sample.duration == 0 {
            require(
                self.zero.replace(sample).is_none(),
                "Multiple zero MP4 sample durations",
            )?;
        }
        self.last = Some(sample);
        Ok(())
    }
}

struct Fragment {
    data_start: u64,
    data_end: u64,
}

async fn inspect_fragment(
    reader: &mut Reader<'_>,
    moof: BoxHeader,
    track: &Track,
    timeline: &mut Timeline,
) -> Result<Fragment> {
    let mut cursor = moof.body;
    let mut traf = None;
    let mut sequence = None;
    while cursor < moof.end {
        let header = reader.header(cursor, moof.end).await?;
        match &header.kind {
            b"mfhd" => {
                require(
                    sequence.is_none()
                        && header.end - header.body == 8
                        && reader.u32(header.body, header.end).await? == 0,
                    "Invalid MP4 fragment sequence",
                )?;
                sequence = Some(reader.u32(header.body + 4, header.end).await?);
            }
            b"traf" => require(
                traf.replace(header).is_none(),
                "Multiple MP4 fragment tracks",
            )?,
            b"free" | b"skip" => {}
            _ => timeline.supported = false,
        }
        cursor = header.end;
    }
    let sequence = sequence.ok_or_else(|| invalid("Missing MP4 fragment sequence"))?;
    if let Some(previous) = timeline.sequence {
        require(
            previous.checked_add(1) == Some(sequence),
            "Unordered MP4 fragments",
        )?;
    }
    timeline.sequence = Some(sequence);
    let traf = traf.ok_or_else(|| invalid("Missing MP4 fragment track"))?;
    let defaults = track.trex.unwrap();
    let mut duration = defaults[2];
    let mut size = defaults[3];
    let mut flags = defaults[4];
    let mut base = moof.start;
    let mut tfhd_seen = false;
    let mut decode = None;
    let mut origin = None;
    let first_ordinal = add(timeline.samples, 1)?;
    let mut runs = 0;
    let mut data_start = None;
    let mut data_end = None;
    cursor = traf.body;
    while cursor < traf.end {
        let header = reader.header(cursor, traf.end).await?;
        let p = header.body;
        match &header.kind {
            b"tfhd" => {
                require(
                    !tfhd_seen && runs == 0,
                    "Duplicate or unordered MP4 fragment header",
                )?;
                tfhd_seen = true;
                let full = reader.u32(p, header.end).await?;
                require(
                    full & !0x02003b == 0 && full & 0x010000 == 0,
                    "Unsupported MP4 fragment flags",
                )?;
                require(
                    reader.u32(p + 4, header.end).await? == track.id.unwrap(),
                    "Conflicting MP4 fragment track",
                )?;
                let mut field = p + 8;
                if full & 1 != 0 {
                    require(full & 0x020000 == 0, "Conflicting MP4 data bases")?;
                    base = reader.u64(field, header.end).await?;
                    field += 8;
                } else {
                    require(full & 0x020000 != 0, "Unsupported implicit MP4 data base")?;
                }
                if full & 2 != 0 {
                    require(
                        reader.u32(field, header.end).await? == 1,
                        "Conflicting MP4 sample description",
                    )?;
                    field += 4;
                }
                for (mask, value) in [(8, &mut duration), (16, &mut size), (32, &mut flags)] {
                    if full & mask != 0 {
                        *value = reader.u32(field, header.end).await?;
                        field += 4;
                    }
                }
                require(field == header.end, "Invalid MP4 fragment header length")?;
            }
            b"tfdt" => {
                require(
                    tfhd_seen && decode.is_none() && runs == 0,
                    "Duplicate or unordered MP4 decode origin",
                )?;
                let full = reader.u32(p, header.end).await?;
                let value = match full {
                    0 => {
                        require(header.end - p == 8, "Invalid MP4 decode origin length")?;
                        u64::from(reader.u32(p + 4, header.end).await?)
                    }
                    0x01000000 => {
                        require(header.end - p == 12, "Invalid MP4 decode origin length")?;
                        reader.u64(p + 4, header.end).await?
                    }
                    _ => return Err(invalid("Unsupported MP4 decode origin")),
                };
                decode = Some(value);
                origin = Some(TimeField {
                    position: p + 4,
                    wide: full != 0,
                    value,
                });
            }
            b"trun" => {
                require(
                    tfhd_seen && decode.is_some(),
                    "Missing MP4 fragment timing header",
                )?;
                runs += 1;
                let full = reader.u32(p, header.end).await?;
                let version = full >> 24;
                let bits = full & 0xffffff;
                require(
                    version <= 1 && bits & !0x000f05 == 0 && !(bits & 4 != 0 && bits & 0x400 != 0),
                    "Unsupported MP4 sample run flags",
                )?;
                let count = reader.u32(p + 4, header.end).await?;
                require(count != 0, "Empty MP4 sample run")?;
                let mut field = p + 8;
                let mut data = if bits & 1 != 0 {
                    let offset = reader.u32(field, header.end).await? as i32;
                    field += 4;
                    if offset < 0 {
                        base.checked_sub(u64::from(offset.unsigned_abs()))
                            .ok_or_else(|| invalid("MP4 data offset underflow"))?
                    } else {
                        add(base, offset as u64)?
                    }
                } else {
                    data_end.ok_or_else(|| invalid("Missing MP4 sample data offset"))?
                };
                if let Some(previous) = data_end {
                    require(previous == data, "Discontiguous MP4 sample data")?;
                }
                data_start.get_or_insert(data);
                let first_flags = if bits & 4 != 0 {
                    let value = reader.u32(field, header.end).await?;
                    field += 4;
                    Some(value)
                } else {
                    None
                };
                let stride = u64::from((bits & 0xf00).count_ones()) * 4;
                require(
                    add(field, u64::from(count) * stride)? == header.end,
                    "Invalid MP4 sample run length",
                )?;
                let mut dts = decode.unwrap();
                let mut index = 0;
                while index < count {
                    let mut sample_duration = duration;
                    let mut sample_size = size;
                    let mut sample_flags = if index == 0 {
                        first_flags.unwrap_or(flags)
                    } else {
                        flags
                    };
                    let mut duration_position = None;
                    if bits & 0x100 != 0 {
                        duration_position = Some(field);
                        sample_duration = reader.u32(field, header.end).await?;
                        field += 4;
                    }
                    if bits & 0x200 != 0 {
                        sample_size = reader.u32(field, header.end).await?;
                        field += 4;
                    }
                    if bits & 0x400 != 0 {
                        sample_flags = reader.u32(field, header.end).await?;
                        field += 4;
                    }
                    require(
                        sample_flags & 0xf0000000 == 0
                            && [20, 22, 24, 26]
                                .into_iter()
                                .all(|shift| (sample_flags >> shift) & 3 != 3),
                        "Invalid MP4 sample flags",
                    )?;
                    let composition = if bits & 0x800 != 0 {
                        let value = reader.u32(field, header.end).await?;
                        field += 4;
                        if version == 1 {
                            i64::from(value as i32)
                        } else {
                            i64::from(value)
                        }
                    } else {
                        0
                    };
                    require(sample_size != 0, "Empty MP4 encoded sample")?;
                    data = add(data, u64::from(sample_size))?;
                    require(data <= reader.length, "MP4 sample data exceeds file")?;
                    timeline.sample(Sample {
                        dts,
                        duration: sample_duration,
                        duration_position,
                        fragment: moof.start,
                        ordinal: 0,
                        composition,
                        origin: if add(timeline.samples, 1)? == first_ordinal {
                            origin
                        } else {
                            None
                        },
                        sync: sample_flags & 0x01010000 == 0
                            && sample_flags & 0x03000000 == 0x02000000,
                    })?;
                    dts = add(dts, u64::from(sample_duration))?;
                    if stride == 0 && count - index > 2 {
                        let skipped = u64::from(count - index - 2);
                        data = add(data, skipped * u64::from(sample_size))?;
                        require(data <= reader.length, "MP4 sample data exceeds file")?;
                        dts = add(dts, skipped * u64::from(sample_duration))?;
                        timeline.samples = add(timeline.samples, skipped)?;
                        index = count - 2;
                    }
                    index += 1;
                }
                decode = Some(dts);
                data_end = Some(data);
            }
            b"free" | b"skip" => {}
            _ => timeline.supported = false,
        }
        cursor = header.end;
    }
    require(runs != 0, "Missing MP4 sample runs")?;
    Ok(Fragment {
        data_start: data_start.unwrap(),
        data_end: data_end.unwrap(),
    })
}

async fn inspect_index(
    reader: &mut Reader<'_>,
    root: BoxHeader,
    track: &Track,
    timeline: &mut Timeline,
) -> Result<()> {
    let mut cursor = root.body;
    let mut tfra_seen = false;
    let mut mfro_seen = false;
    while cursor < root.end {
        let header = reader.header(cursor, root.end).await?;
        let p = header.body;
        match &header.kind {
            b"tfra" => {
                require(
                    !tfra_seen && !mfro_seen,
                    "Duplicate or unordered MP4 fragment index",
                )?;
                tfra_seen = true;
                let version = reader.u32(p, header.end).await?;
                require(
                    version == 0 || version == 0x01000000,
                    "Unsupported MP4 fragment index version",
                )?;
                require(
                    reader.u32(p + 4, header.end).await? == track.id.unwrap(),
                    "Conflicting MP4 fragment index track",
                )?;
                let lengths = reader.u32(p + 8, header.end).await?;
                require(lengths & !63 == 0, "Invalid MP4 fragment index lengths")?;
                let widths = [
                    ((lengths >> 4) & 3) + 1,
                    ((lengths >> 2) & 3) + 1,
                    (lengths & 3) + 1,
                ];
                let count = reader.u32(p + 12, header.end).await?;
                let wide = if version == 0 { 4 } else { 8 };
                let stride = wide * 2 + u64::from(widths.iter().sum::<u32>());
                let mut field = p + 16;
                require(
                    add(field, u64::from(count) * stride)? == header.end,
                    "Invalid MP4 fragment index length",
                )?;
                let mut top_cursor = 0;
                let mut previous_time = None;
                let mut previous_offset = None;
                for _ in 0..count {
                    let time_position = field;
                    let time = if wide == 8 {
                        reader.u64(field, header.end).await?
                    } else {
                        u64::from(reader.u32(field, header.end).await?)
                    };
                    field += wide;
                    let offset = if wide == 8 {
                        reader.u64(field, header.end).await?
                    } else {
                        u64::from(reader.u32(field, header.end).await?)
                    };
                    field += wide;
                    let is_boundary = timeline
                        .boundary
                        .is_some_and(|sample| sample.fragment == offset && sample.dts == time);
                    require(
                        previous_time.is_none_or(|previous| {
                            time > previous || (time == previous && is_boundary)
                        }) && previous_offset.is_none_or(|previous| offset > previous),
                        "Unordered MP4 fragment index",
                    )?;
                    previous_time = Some(time);
                    previous_offset = Some(offset);
                    while top_cursor < offset {
                        top_cursor = reader.header(top_cursor, root.start).await?.end;
                    }
                    require(
                        top_cursor == offset,
                        "MP4 fragment index points inside a box",
                    )?;
                    let moof = reader.header(offset, root.start).await?;
                    require(
                        moof.kind == *b"moof",
                        "MP4 fragment index does not reference a fragment",
                    )?;
                    let mut references = [0u32; 3];
                    for (value, width) in references.iter_mut().zip(widths) {
                        for _ in 0..width {
                            *value = (*value << 8)
                                | u32::from(reader.bytes::<1>(field, header.end).await?[0]);
                            field += 1;
                        }
                        require(*value != 0, "Invalid MP4 fragment index sample")?;
                    }
                    if is_boundary {
                        require(
                            references == [1, 1, 1] && timeline.boundary_index.is_none(),
                            "Unsafe terminal MP4 fragment index",
                        )?;
                        timeline.boundary_index = Some(TimeField {
                            position: time_position,
                            wide: wide == 8,
                            value: time,
                        });
                    }
                    if references != [1, 1, 1] {
                        timeline.supported = false;
                    } else {
                        let mut child = moof.body;
                        let mut found = false;
                        while child < moof.end {
                            let nested = reader.header(child, moof.end).await?;
                            if nested.kind == *b"traf" {
                                let mut inner = nested.body;
                                while inner < nested.end {
                                    let origin = reader.header(inner, nested.end).await?;
                                    if origin.kind == *b"tfdt" {
                                        let full = reader.u32(origin.body, origin.end).await?;
                                        let value = if full == 0 {
                                            u64::from(
                                                reader.u32(origin.body + 4, origin.end).await?,
                                            )
                                        } else {
                                            reader.u64(origin.body + 4, origin.end).await?
                                        };
                                        require(
                                            value == time,
                                            "Conflicting MP4 fragment index timestamp",
                                        )?;
                                        found = true;
                                    }
                                    inner = origin.end;
                                }
                            }
                            child = nested.end;
                        }
                        require(found, "Missing indexed MP4 decode origin")?;
                    }
                }
            }
            b"mfro" => {
                require(
                    !mfro_seen
                        && header.end == root.end
                        && header.end - p == 8
                        && reader.u32(p, header.end).await? == 0
                        && u64::from(reader.u32(p + 4, header.end).await?) == root.end - root.start,
                    "Invalid MP4 fragment index footer",
                )?;
                mfro_seen = true;
            }
            _ => timeline.supported = false,
        }
        cursor = header.end;
    }
    require(tfra_seen && mfro_seen, "Incomplete MP4 fragment index")
}

pub(super) async fn repair(file: &mut File) -> Result<()> {
    let length = file.metadata().await?.len();
    let mut reader = Reader {
        file,
        length,
        cache: [0; 8192],
        cache_start: 0,
        cache_end: 0,
    };
    let mut cursor = 0;
    let mut track = None;
    let mut ftyp_seen = false;
    let mut index_seen = false;
    let mut pending = None;
    let mut timeline = Timeline {
        supported: true,
        ..Timeline::default()
    };
    while cursor < length {
        let header = reader.header(cursor, length).await?;
        match &header.kind {
            b"ftyp" => {
                require(
                    !ftyp_seen
                        && cursor == 0
                        && header.end - header.body >= 8
                        && (header.end - header.body) % 4 == 0,
                    "Invalid MP4 file type",
                )?;
                ftyp_seen = true;
            }
            b"moov" => {
                require(
                    ftyp_seen && track.is_none() && timeline.samples == 0,
                    "Duplicate or unordered MP4 movie metadata",
                )?;
                track = Some(inspect_metadata(&mut reader, header).await?);
            }
            b"moof" => {
                require(
                    pending.is_none() && !index_seen,
                    "Unordered MP4 media fragments",
                )?;
                let track = track
                    .as_ref()
                    .ok_or_else(|| invalid("Missing MP4 movie metadata"))?;
                pending = Some(inspect_fragment(&mut reader, header, track, &mut timeline).await?);
            }
            b"mdat" => {
                let fragment = pending
                    .take()
                    .ok_or_else(|| invalid("Unreferenced MP4 media data"))?;
                require(
                    fragment.data_start == header.body && fragment.data_end == header.end,
                    "Unsafe MP4 sample data range",
                )?;
            }
            b"mfra" => {
                require(
                    pending.is_none()
                        && timeline.samples != 0
                        && !index_seen
                        && header.end == length,
                    "Unordered MP4 fragment index",
                )?;
                inspect_index(&mut reader, header, track.as_ref().unwrap(), &mut timeline).await?;
                index_seen = true;
            }
            b"free" | b"skip" => {}
            _ => timeline.supported = false,
        }
        cursor = header.end;
    }
    require(
        ftyp_seen && track.is_some() && timeline.samples != 0 && pending.is_none(),
        "Incomplete MP4 recording",
    )?;
    if timeline.zero.is_some() || timeline.boundary.is_some() {
        let track = track.as_ref().unwrap();
        require(
            track.supported && track.video && track.av1 && timeline.supported,
            "Unsupported metadata for MP4 timing repair",
        )?;
        require(
            !(timeline.zero.is_some() && timeline.boundary.is_some()),
            "Multiple MP4 timing defects",
        )?;
    }
    if let Some(zero) = timeline.zero {
        let last = timeline.last.unwrap();
        require(timeline.boundary.is_none(), "Multiple MP4 timing defects")?;
        require(
            add(zero.ordinal, 1)? == last.ordinal
                && zero.fragment == last.fragment
                && zero.dts == last.dts
                && zero.composition == 0
                && last.composition == 0,
            "Zero duration is not the terminal MP4 boundary",
        )?;
        require(
            last.duration >= 2,
            "Insufficient terminal MP4 duration for repair",
        )?;
        let zero_position = zero
            .duration_position
            .ok_or_else(|| invalid("Zero MP4 duration is not explicit"))?;
        let last_position = last
            .duration_position
            .ok_or_else(|| invalid("Terminal MP4 duration is not explicit"))?;
        require(
            zero_position < last_position,
            "Unordered MP4 duration fields",
        )?;
        add(last.dts, 1)?;
        reader.file.seek(SeekFrom::Start(zero_position)).await?;
        reader.file.write_all(&1u32.to_be_bytes()).await?;
        reader.file.seek(SeekFrom::Start(last_position)).await?;
        reader
            .file
            .write_all(&(last.duration - 1).to_be_bytes())
            .await?;
    }
    if let Some(boundary) = timeline.boundary {
        let last = timeline.last.unwrap();
        require(
            boundary.ordinal == last.ordinal && last.duration >= 2 && last.composition == 0,
            "Duplicate timestamp is not the terminal single-sample MP4 fragment",
        )?;
        let origin = last
            .origin
            .ok_or_else(|| invalid("Missing terminal MP4 decode origin"))?
            .incremented()?;
        let index = timeline
            .boundary_index
            .map(TimeField::incremented)
            .transpose()?;
        let duration_position = last
            .duration_position
            .ok_or_else(|| invalid("Terminal MP4 duration is not explicit"))?;
        origin.write(reader.file).await?;
        reader.file.seek(SeekFrom::Start(duration_position)).await?;
        reader
            .file
            .write_all(&(last.duration - 1).to_be_bytes())
            .await?;
        if let Some(index) = index {
            index.write(reader.file).await?;
        }
    }
    Ok(())
}

#[cfg(test)]
pub(super) const REPAIRABLE_MP4: &[u8] =
    include_bytes!("../../tests/fixtures/recordings/tail-129.mp4");

#[cfg(test)]
const fn valid_fixture() -> [u8; 1669] {
    let mut bytes = *include_bytes!("../../tests/fixtures/recordings/tail-129.mp4");
    bytes[845] = 1;
    bytes[853] = 0xe6;
    bytes
}

#[cfg(test)]
pub(super) const VALID_MP4: &[u8] = &valid_fixture();

#[cfg(test)]
mod tests;
