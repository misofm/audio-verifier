#![allow(dead_code)]

use std::{
    env,
    path::{Path, PathBuf},
    process::Command,
};

use md5::{Digest as _, Md5};

pub fn fixture_path(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

pub fn fixture(name: &str) -> Vec<u8> {
    std::fs::read(fixture_path(name)).expect("fixture")
}

pub fn committed_blob_id(name: &str) -> String {
    let listing = std::fs::read_to_string(fixture_path("walrus-blob-ids.txt")).expect("listing");
    listing
        .lines()
        .find_map(|line| {
            let (file, blob_id) = line.split_once(' ')?;
            (file == name).then(|| blob_id.to_owned())
        })
        .expect("blob ID listed")
}

pub fn ffmpeg() -> Option<PathBuf> {
    let candidate = env::var_os("FFMPEG")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/usr/bin/ffmpeg"));
    Command::new(&candidate)
        .arg("-version")
        .output()
        .ok()
        .filter(|output| output.status.success())
        .map(|_| candidate)
}

pub fn walrus_cli() -> Option<PathBuf> {
    let candidate = PathBuf::from(env::var_os("WALRUS_CLI")?);
    candidate.exists().then_some(candidate)
}

pub fn scratch_file(name: &str, bytes: &[u8]) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("audio-verifier");
    std::fs::create_dir_all(&dir).expect("scratch dir");
    let path = dir.join(name);
    std::fs::write(&path, bytes).expect("scratch file");
    path
}

pub fn ffmpeg_raw_pcm(ffmpeg: &Path, flac: &Path, bit_depth: u8) -> Vec<u8> {
    let format = match bit_depth {
        8 => "s8",
        16 => "s16le",
        24 => "s24le",
        other => panic!("no raw format for {other}-bit"),
    };
    let codec = match bit_depth {
        8 => "pcm_s8",
        16 => "pcm_s16le",
        _ => "pcm_s24le",
    };
    let output = Command::new(ffmpeg)
        .args([
            "-hide_banner",
            "-loglevel",
            "error",
            "-threads",
            "1",
            "-filter_threads",
            "1",
            "-i",
        ])
        .arg(flac)
        .args(["-f", format, "-c:a", codec, "-"])
        .output()
        .expect("ffmpeg runs");
    assert!(output.status.success(), "ffmpeg decode failed");
    output.stdout
}

pub fn walrus_cli_blob_id(cli: &Path, file: &Path) -> String {
    let output = Command::new(cli)
        .args(["blob-id", "--n-shards", "1000", "--json"])
        .arg(file)
        .output()
        .expect("walrus CLI runs");
    assert!(output.status.success(), "walrus blob-id failed");
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).expect("CLI JSON");
    json["blobId"].as_str().expect("blobId").to_owned()
}

pub fn set_streaminfo(
    bytes: &mut [u8],
    sample_rate_hz: u32,
    channels: u8,
    bit_depth: u8,
    total_samples: u64,
) {
    let packed = (u64::from(sample_rate_hz) << 44)
        | (u64::from(channels - 1) << 41)
        | (u64::from(bit_depth - 1) << 36)
        | total_samples;
    bytes[18..26].copy_from_slice(&packed.to_be_bytes());
}

pub fn set_block_sizes(bytes: &mut [u8], min: u16, max: u16) {
    bytes[8..10].copy_from_slice(&min.to_be_bytes());
    bytes[10..12].copy_from_slice(&max.to_be_bytes());
}

pub fn set_frame_sizes(bytes: &mut [u8], min: u32, max: u32) {
    bytes[12..15].copy_from_slice(&min.to_be_bytes()[1..]);
    bytes[15..18].copy_from_slice(&max.to_be_bytes()[1..]);
}

pub fn block_sizes(bytes: &[u8]) -> (u16, u16) {
    (
        u16::from_be_bytes([bytes[8], bytes[9]]),
        u16::from_be_bytes([bytes[10], bytes[11]]),
    )
}

pub fn frame_sizes(bytes: &[u8]) -> (u32, u32) {
    (
        u32::from_be_bytes([0, bytes[12], bytes[13], bytes[14]]),
        u32::from_be_bytes([0, bytes[15], bytes[16], bytes[17]]),
    )
}

pub fn set_md5(bytes: &mut [u8], md5: [u8; 16]) {
    bytes[26..42].copy_from_slice(&md5);
}

pub fn decoded_samples(bytes: &[u8]) -> Vec<i32> {
    struct Interleave(Vec<i32>);

    impl libflac::Handler for Interleave {
        type Error = std::convert::Infallible;

        fn stream_info(&mut self, _info: &libflac::StreamInfo) -> Result<(), Self::Error> {
            Ok(())
        }

        fn frame(&mut self, frame: &libflac::Frame<'_>) -> Result<(), Self::Error> {
            for i in 0..frame.block_size as usize {
                self.0
                    .extend(frame.samples.iter().map(|channel| channel[i]));
            }
            Ok(())
        }
    }

    let mut samples = Interleave(Vec::new());
    libflac::decode(bytes, &mut samples).expect("decode");
    samples.0
}

pub fn pcm_md5(samples: &[i32], bytes_per_sample: usize) -> [u8; 16] {
    let mut md5 = Md5::new();
    for sample in samples {
        md5.update(&sample.to_le_bytes()[..bytes_per_sample]);
    }
    md5.finalize().into()
}

#[derive(Debug, Clone, Copy)]
pub enum Rate {
    FromStreamInfo,
    Code(u8),
    Khz(u8),
    Hz(u16),
    TensOfHz(u16),
}

impl Rate {
    pub fn standard(sample_rate_hz: u32) -> Self {
        Self::Code(match sample_rate_hz {
            88_200 => 1,
            176_400 => 2,
            192_000 => 3,
            22_050 => 6,
            44_100 => 9,
            48_000 => 10,
            96_000 => 11,
            other => panic!("no standard code for {other}"),
        })
    }

    fn code_and_extra(self) -> (u8, Vec<u8>) {
        match self {
            Self::FromStreamInfo => (0, vec![]),
            Self::Code(code) => (code, vec![]),
            Self::Khz(khz) => (0b1100, vec![khz]),
            Self::Hz(hz) => (0b1101, hz.to_be_bytes().to_vec()),
            Self::TensOfHz(tens) => (0b1110, tens.to_be_bytes().to_vec()),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct FrameHeader {
    pub channels: u8,
    pub bit_depth: u8,
    pub rate: Rate,
}

#[derive(Debug, Clone)]
pub struct Synth {
    pub channels: u8,
    pub bit_depth: u8,
    pub sample_rate_hz: u32,
    pub frames_per_channel: usize,
    pub block_size: usize,
    pub variable_block_sizes: Vec<usize>,
    pub frame_header: FrameHeader,
    pub override_from_block: usize,
    pub frame_edits: Vec<(usize, FrameEdit)>,
}

/// A deliberate fault in one frame's header; CRCs are computed after it.
#[derive(Debug, Clone)]
pub enum FrameEdit {
    /// Replaces the coded frame or sample number with these bytes.
    CodedNumber(Vec<u8>),
    /// Flips the blocking strategy bit, keeping the coded number.
    FlipBlocking,
}

impl Synth {
    pub fn new(channels: u8, bit_depth: u8, sample_rate_hz: u32) -> Self {
        Self {
            channels,
            bit_depth,
            sample_rate_hz,
            frames_per_channel: 2_500,
            block_size: 1_024,
            variable_block_sizes: Vec::new(),
            frame_header: FrameHeader {
                channels,
                bit_depth,
                rate: Rate::standard(sample_rate_hz),
            },
            override_from_block: 0,
            frame_edits: Vec::new(),
        }
    }

    pub fn with_frame_edit(mut self, frame: usize, edit: FrameEdit) -> Self {
        self.frame_edits.push((frame, edit));
        self
    }

    pub fn with_header(mut self, frame_header: FrameHeader) -> Self {
        self.frame_header = frame_header;
        self
    }

    pub fn with_variable_block_sizes(mut self, block_sizes: &[usize]) -> Self {
        self.frames_per_channel = block_sizes.iter().sum();
        self.variable_block_sizes = block_sizes.to_vec();
        self
    }

    pub fn build(&self) -> Vec<u8> {
        let honest = FrameHeader {
            channels: self.channels,
            bit_depth: self.bit_depth,
            rate: Rate::standard(self.sample_rate_hz),
        };
        let bytes_per_sample = usize::from(self.bit_depth / 8);
        let variable = !self.variable_block_sizes.is_empty();
        let mut frames = Vec::new();
        let mut durations = Vec::new();
        let mut frame_sizes = Vec::new();
        let mut decoded = Vec::new();
        let mut offset = 0;
        let mut index = 0;
        while offset < self.frames_per_channel {
            let header = if index >= self.override_from_block {
                self.frame_header
            } else {
                honest
            };
            let len = if variable {
                self.variable_block_sizes[index]
            } else {
                self.block_size.min(self.frames_per_channel - offset)
            };
            let channels: Vec<Vec<i32>> = (0..header.channels)
                .map(|ch| {
                    (0..len)
                        .map(|i| sample_value(offset + i, ch, header.bit_depth))
                        .collect()
                })
                .collect();
            for i in 0..len {
                for channel in &channels {
                    decoded.push(channel[i]);
                }
            }
            let number = if variable { offset } else { index };
            let mut coded = utf8_number(number as u32);
            let mut frame_variable = variable;
            for (_, edit) in self.frame_edits.iter().filter(|(at, _)| *at == index) {
                match edit {
                    FrameEdit::CodedNumber(bytes) => coded.clone_from(bytes),
                    FrameEdit::FlipBlocking => frame_variable = !frame_variable,
                }
            }
            let encoded = frame(frame_variable, &coded, header, &channels);
            durations.push(len);
            frame_sizes.push(encoded.len() as u32);
            frames.extend(encoded);
            offset += len;
            index += 1;
        }
        let total = decoded.len() as u64 / u64::from(self.channels);
        let mut out = b"fLaC".to_vec();
        out.extend([0x80, 0, 0, 34]);
        out.extend([0; 10]);
        out.extend([0; 8]);
        out.extend([0; 16]);
        set_streaminfo(
            &mut out,
            self.sample_rate_hz,
            self.channels,
            self.bit_depth,
            total,
        );
        set_md5(&mut out, pcm_md5(&decoded, bytes_per_sample));
        let (min_block, max_block) = if variable {
            let before_last = &durations[..durations.len() - 1];
            (
                before_last.iter().copied().min().unwrap_or(durations[0]),
                durations.iter().copied().max().expect("a frame"),
            )
        } else {
            (self.block_size, self.block_size)
        };
        set_block_sizes(&mut out, min_block as u16, max_block as u16);
        set_frame_sizes(
            &mut out,
            frame_sizes.iter().copied().min().expect("a frame"),
            frame_sizes.iter().copied().max().expect("a frame"),
        );
        out.extend(frames);
        out
    }
}

fn sample_value(index: usize, channel: u8, bit_depth: u8) -> i32 {
    let range = 1i64 << bit_depth;
    let value = (index as i64 * 7_919 + i64::from(channel) * 104_729) % range;
    (value - range / 2) as i32
}

fn bps_code(bit_depth: u8) -> u8 {
    match bit_depth {
        8 => 0b001,
        12 => 0b010,
        16 => 0b100,
        20 => 0b101,
        24 => 0b110,
        other => panic!("no frame code for {other}-bit"),
    }
}

fn frame(
    variable: bool,
    coded_number: &[u8],
    header: FrameHeader,
    channels: &[Vec<i32>],
) -> Vec<u8> {
    let block_size = channels[0].len();
    let (rate_code, rate_extra) = header.rate.code_and_extra();
    let sync = if variable { 0xf9 } else { 0xf8 };
    let mut out = vec![0xff, sync, (0b0111 << 4) | rate_code];
    out.push(((header.channels - 1) << 4) | (bps_code(header.bit_depth) << 1));
    out.extend(coded_number);
    out.extend(((block_size - 1) as u16).to_be_bytes());
    out.extend(rate_extra);
    out.push(crc8(&out));

    let mut bits = BitWriter::default();
    for channel in channels {
        bits.write(0b0000_0010, 8);
        for sample in channel {
            bits.write(*sample as u32 as u64, u32::from(header.bit_depth));
        }
    }
    out.extend(bits.finish());
    let crc = crc16(&out);
    out.extend(crc.to_be_bytes());
    out
}

pub fn utf8_number(number: u32) -> Vec<u8> {
    match number {
        0..0x80 => vec![number as u8],
        0x80..0x800 => vec![0xc0 | (number >> 6) as u8, 0x80 | (number & 0x3f) as u8],
        _ => vec![
            0xe0 | (number >> 12) as u8,
            0x80 | ((number >> 6) & 0x3f) as u8,
            0x80 | (number & 0x3f) as u8,
        ],
    }
}

#[derive(Default)]
struct BitWriter {
    bytes: Vec<u8>,
    current: u64,
    filled: u32,
}

impl BitWriter {
    fn write(&mut self, value: u64, bits: u32) {
        for shift in (0..bits).rev() {
            self.current = (self.current << 1) | ((value >> shift) & 1);
            self.filled += 1;
            if self.filled == 8 {
                self.bytes.push(self.current as u8);
                self.current = 0;
                self.filled = 0;
            }
        }
    }

    fn finish(mut self) -> Vec<u8> {
        if self.filled > 0 {
            self.bytes.push((self.current << (8 - self.filled)) as u8);
        }
        self.bytes
    }
}

fn crc8(bytes: &[u8]) -> u8 {
    let mut crc = 0u8;
    for byte in bytes {
        crc ^= byte;
        for _ in 0..8 {
            crc = if crc & 0x80 != 0 {
                (crc << 1) ^ 0x07
            } else {
                crc << 1
            };
        }
    }
    crc
}

fn crc16(bytes: &[u8]) -> u16 {
    let mut crc = 0u16;
    for byte in bytes {
        crc ^= u16::from(*byte) << 8;
        for _ in 0..8 {
            crc = if crc & 0x8000 != 0 {
                (crc << 1) ^ 0x8005
            } else {
                crc << 1
            };
        }
    }
    crc
}
