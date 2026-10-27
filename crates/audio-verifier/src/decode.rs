use std::sync::mpsc;
use std::{panic, thread};

use libflac::{Frame, Handler};
use md5::{Digest as _, Md5};

use crate::frame_header::{self, FrameHeader};
use crate::{Error, StreamInfo};

const PIPELINE_DEPTH: usize = 4;

pub(crate) fn verify_and_digest(bytes: &[u8], info: &StreamInfo) -> Result<[u8; 32], Error> {
    let (full, filled) = mpsc::sync_channel::<Vec<u8>>(PIPELINE_DEPTH);
    let (empty, recycled) = mpsc::sync_channel::<Vec<u8>>(PIPELINE_DEPTH);
    for _ in 0..PIPELINE_DEPTH {
        empty.send(Vec::new()).expect("recycled channel has room");
    }

    let (checked, (decoded_md5, pcm_digest)) = thread::scope(|scope| {
        let hasher = scope.spawn(move || {
            let mut md5 = Md5::new();
            let mut blake3 = blake3::Hasher::new();
            for pcm in filled {
                md5.update(&pcm);
                blake3.update(&pcm);
                if empty.send(pcm).is_err() {
                    break;
                }
            }
            let md5: [u8; 16] = md5.finalize().into();
            (md5, *blake3.finalize().as_bytes())
        });
        let mut checker = Canonical::new(info, &full, &recycled);
        let checked = match libflac::decode(bytes, &mut checker) {
            Ok(summary) => checker.finish(bytes, summary),
            Err(libflac::Error::Handler(error)) => Err(error),
            // libFLAC reports a gap only after reading the next frame in full,
            // where the previous frame's block size is checked first.
            Err(
                error @ libflac::Error::Stream {
                    status: libflac::ErrorStatus::MissingFrame,
                    ..
                },
            ) => checker
                .check_previous_block()
                .and(Err(Error::Decode(error.to_string()))),
            Err(error) => Err(Error::Decode(error.to_string())),
        };
        drop(full);
        let digests = hasher
            .join()
            .unwrap_or_else(|panic| panic::resume_unwind(panic));
        (checked, digests)
    });

    let samples = checked?;
    if samples != info.total_samples {
        return Err(Error::SampleCount {
            expected: info.total_samples,
            decoded: samples,
        });
    }
    if decoded_md5 != info.md5 {
        return Err(Error::Md5Mismatch {
            expected: hex(&info.md5),
            decoded: hex(&decoded_md5),
        });
    }

    Ok(pcm_digest)
}

/// Checks every frame against STREAMINFO and sends its packed PCM to the hasher.
struct Canonical<'a> {
    info: &'a StreamInfo,
    full: &'a mpsc::SyncSender<Vec<u8>>,
    recycled: &'a mpsc::Receiver<Vec<u8>>,
    variable: Option<bool>,
    frames: u64,
    samples: u64,
    last_block: u32,
    min_block_before_last: u32,
    max_block: u32,
    min_frame: u64,
    max_frame: u64,
}

impl<'a> Canonical<'a> {
    fn new(
        info: &'a StreamInfo,
        full: &'a mpsc::SyncSender<Vec<u8>>,
        recycled: &'a mpsc::Receiver<Vec<u8>>,
    ) -> Self {
        Self {
            info,
            full,
            recycled,
            variable: None,
            frames: 0,
            samples: 0,
            last_block: 0,
            min_block_before_last: u32::MAX,
            max_block: 0,
            min_frame: u64::MAX,
            max_frame: 0,
        }
    }

    /// Checks a frame against STREAMINFO and the frames before it.
    fn check(&self, frame: &Frame<'_>) -> Result<FrameHeader, Error> {
        let index = frame.index;
        let header = frame_header::parse(frame.bytes).map_err(|reason| {
            Error::Decode(format!("frame {index} at byte {}: {reason}", frame.offset))
        })?;
        if header.block_size != frame.block_size {
            return Err(Error::Decode(format!(
                "frame {index} header has {} samples, libFLAC decoded {}",
                header.block_size, frame.block_size
            )));
        }
        if self
            .variable
            .is_some_and(|variable| variable != header.variable)
        {
            return Err(Error::Decode(format!(
                "frame {index} changes the blocking strategy"
            )));
        }
        let info = self.info;
        if frame.channels != u32::from(info.channels) {
            return Err(Error::FrameChannels {
                frame: index,
                expected: info.channels,
                found: frame.channels,
            });
        }
        if frame.bits_per_sample != u32::from(info.bit_depth) {
            return Err(Error::FrameBitDepth {
                frame: index,
                expected: info.bit_depth,
                found: frame.bits_per_sample,
            });
        }
        if frame.sample_rate != info.sample_rate_hz {
            return Err(Error::FrameSampleRate {
                frame: index,
                expected: info.sample_rate_hz,
                found: frame.sample_rate,
            });
        }
        let decoded = self.samples + u64::from(frame.block_size);
        if decoded > info.total_samples {
            return Err(Error::SampleCount {
                expected: info.total_samples,
                decoded,
            });
        }
        self.check_previous_block()?;
        if frame.block_size > u32::from(info.max_block_size) {
            return Err(Error::FrameBlockSizeAboveMax {
                frame: index,
                expected: info.max_block_size,
                found: frame.block_size,
            });
        }
        if header.variable && header.number != self.samples {
            return Err(Error::Decode(format!(
                "frame {index} starts at sample {}, expected {}",
                header.number, self.samples
            )));
        }
        if !header.variable && header.number != index {
            return Err(Error::Decode(format!(
                "frame {index} has frame number {}",
                header.number
            )));
        }
        let frame_size = frame.bytes.len() as u64;
        if info.max_frame_size != 0 && frame_size > u64::from(info.max_frame_size) {
            return Err(Error::MaxFrameSize {
                expected: info.max_frame_size,
                found: frame_size,
            });
        }
        Ok(header)
    }

    /// A frame was followed by another, so it was not the last and must reach
    /// the STREAMINFO minimum block size.
    fn check_previous_block(&self) -> Result<(), Error> {
        if self.frames > 0 && self.last_block < u32::from(self.info.min_block_size) {
            return Err(Error::FrameBlockSizeBelowMin {
                frame: self.frames - 1,
                expected: self.info.min_block_size,
                found: self.last_block,
            });
        }
        Ok(())
    }

    fn observe(&mut self, frame: &Frame<'_>, variable: bool) {
        if self.frames > 0 {
            self.min_block_before_last = self.min_block_before_last.min(self.last_block);
        }
        let frame_size = frame.bytes.len() as u64;
        self.variable = Some(variable);
        self.frames += 1;
        self.samples += u64::from(frame.block_size);
        self.last_block = frame.block_size;
        self.max_block = self.max_block.max(frame.block_size);
        self.min_frame = self.min_frame.min(frame_size);
        self.max_frame = self.max_frame.max(frame_size);
    }

    /// The end-of-stream checks. Returns the decoded samples per channel.
    fn finish(&self, bytes: &[u8], summary: libflac::Summary) -> Result<u64, Error> {
        if summary.end != bytes.len() as u64 {
            return Err(Error::Decode(format!(
                "{} bytes after the last complete frame at byte {}",
                bytes.len() as u64 - summary.end,
                summary.end
            )));
        }
        let info = self.info;
        if self.frames >= 2 {
            if self.min_block_before_last != u32::from(info.min_block_size) {
                return Err(Error::MinBlockSize {
                    expected: info.min_block_size,
                    found: self.min_block_before_last,
                });
            }
            if self.max_block != u32::from(info.max_block_size) {
                return Err(Error::MaxBlockSize {
                    expected: info.max_block_size,
                    found: self.max_block,
                });
            }
        }
        if self.frames > 0 {
            if info.min_frame_size != 0 && u64::from(info.min_frame_size) != self.min_frame {
                return Err(Error::MinFrameSize {
                    expected: info.min_frame_size,
                    found: self.min_frame,
                });
            }
            if info.max_frame_size != 0 && u64::from(info.max_frame_size) != self.max_frame {
                return Err(Error::MaxFrameSize {
                    expected: info.max_frame_size,
                    found: self.max_frame,
                });
            }
        }
        Ok(self.samples)
    }

    fn send_pcm(&self, frame: &Frame<'_>) {
        let mut buffer = self.recycled.recv().expect("hasher returns every buffer");
        match self.info.bytes_per_sample() {
            1 => pack::<1>(frame.samples, &mut buffer),
            2 => pack::<2>(frame.samples, &mut buffer),
            3 => pack::<3>(frame.samples, &mut buffer),
            other => unreachable!("validated bit depths pack to 1 to 3 bytes, not {other}"),
        }
        self.full.send(buffer).expect("hasher outlives the decoder");
    }
}

impl Handler for Canonical<'_> {
    type Error = Error;

    fn stream_info(&mut self, decoded: &libflac::StreamInfo) -> Result<(), Error> {
        let info = self.info;
        if decoded.sample_rate != info.sample_rate_hz
            || decoded.channels != u32::from(info.channels)
            || decoded.bits_per_sample != u32::from(info.bit_depth)
            || decoded.total_samples != info.total_samples
            || decoded.md5 != info.md5
            || decoded.min_block_size != u32::from(info.min_block_size)
            || decoded.max_block_size != u32::from(info.max_block_size)
            || decoded.min_frame_size != info.min_frame_size
            || decoded.max_frame_size != info.max_frame_size
        {
            return Err(Error::StreamInfoMismatch);
        }
        Ok(())
    }

    fn frame(&mut self, frame: &Frame<'_>) -> Result<(), Error> {
        let header = self.check(frame)?;
        self.observe(frame, header.variable);
        self.send_pcm(frame);
        Ok(())
    }
}

/// Interleaves `channels` by sample frame into `out`, each sample signed
/// little-endian in `B` bytes.
fn pack<const B: usize>(channels: &[&[i32]], out: &mut Vec<u8>) {
    let block_size = channels.first().map_or(0, |channel| channel.len());
    out.clear();
    out.resize(block_size * channels.len() * B, 0);
    // Stereo, by far the common case, gets a loop the compiler can keep tight.
    if let [left, right] = channels {
        let frames = out
            .chunks_exact_mut(2 * B)
            .zip(left.iter().zip(right.iter()));
        for (frame, (left, right)) in frames {
            frame[..B].copy_from_slice(&left.to_le_bytes()[..B]);
            frame[B..].copy_from_slice(&right.to_le_bytes()[..B]);
        }
        return;
    }
    for (i, frame) in out.chunks_exact_mut(channels.len() * B).enumerate() {
        for (channel, sample) in channels.iter().zip(frame.chunks_exact_mut(B)) {
            sample.copy_from_slice(&channel[i].to_le_bytes()[..B]);
        }
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}
