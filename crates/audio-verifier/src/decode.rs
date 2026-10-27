use std::sync::mpsc;
use std::{panic, thread};

use flac_decoder::{Config, Decoder, ErrorKind, FrameInfo, PcmSink};
use md5::{Digest as _, Md5};

use crate::{Error, StreamInfo};

const PIPELINE_DEPTH: usize = 4;

pub(crate) fn verify_and_digest(bytes: &[u8], info: &StreamInfo) -> Result<[u8; 32], Error> {
    let mut decoder = Decoder::with_config(bytes, Config::canonical()).map_err(decode_error)?;
    let decoder_info = decoder.stream_info();
    if decoder_info.sample_rate != info.sample_rate_hz
        || decoder_info.channels != info.channels
        || decoder_info.bits_per_sample != info.bit_depth
        || decoder_info.total_samples != info.total_samples
        || decoder_info.md5 != info.md5
        || decoder_info.min_block_size != info.min_block_size
        || decoder_info.max_block_size != info.max_block_size
        || decoder_info.min_frame_size != info.min_frame_size
        || decoder_info.max_frame_size != info.max_frame_size
    {
        return Err(Error::StreamInfoMismatch);
    }

    let (full, filled) = mpsc::sync_channel::<Vec<u8>>(PIPELINE_DEPTH);
    let (empty, recycled) = mpsc::sync_channel::<Vec<u8>>(PIPELINE_DEPTH);
    for _ in 0..PIPELINE_DEPTH {
        empty.send(Vec::new()).expect("recycled channel has room");
    }

    let (decoded, (decoded_md5, pcm_digest)) = thread::scope(|scope| {
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
        let mut sink = Hashing {
            full: &full,
            recycled: &recycled,
        };
        let decoded = decoder.decode_all(&mut sink).map_err(decode_error);
        drop(full);
        let digests = hasher
            .join()
            .unwrap_or_else(|panic| panic::resume_unwind(panic));
        (decoded, digests)
    });

    let samples = decoded?.samples;
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

struct Hashing<'a> {
    full: &'a mpsc::SyncSender<Vec<u8>>,
    recycled: &'a mpsc::Receiver<Vec<u8>>,
}

impl PcmSink for Hashing<'_> {
    type Error = std::convert::Infallible;

    fn write(&mut self, _info: &FrameInfo, pcm: &[u8]) -> Result<(), Self::Error> {
        let mut buffer = self.recycled.recv().expect("hasher returns every buffer");
        buffer.clear();
        buffer.extend_from_slice(pcm);
        self.full.send(buffer).expect("hasher outlives the decoder");
        Ok(())
    }
}

fn decode_error(error: flac_decoder::Error) -> Error {
    let frame = error.position.frame.unwrap_or(0);
    match error.kind {
        ErrorKind::FrameChannels { expected, found } => Error::FrameChannels {
            frame,
            expected,
            found: u32::from(found),
        },
        ErrorKind::FrameSampleRate { expected, found } => Error::FrameSampleRate {
            frame,
            expected,
            found,
        },
        ErrorKind::FrameBitDepth { expected, found } => Error::FrameBitDepth {
            frame,
            expected,
            found: u32::from(found),
        },
        ErrorKind::FrameBlockSizeBelowMin { expected, found } => Error::FrameBlockSizeBelowMin {
            frame,
            expected,
            found,
        },
        ErrorKind::FrameBlockSizeAboveMax { expected, found } => Error::FrameBlockSizeAboveMax {
            frame,
            expected,
            found,
        },
        ErrorKind::MinBlockSize { expected, found } => Error::MinBlockSize { expected, found },
        ErrorKind::MaxBlockSize { expected, found } => Error::MaxBlockSize { expected, found },
        ErrorKind::MinFrameSize { expected, found } => Error::MinFrameSize { expected, found },
        ErrorKind::MaxFrameSize { expected, found } => Error::MaxFrameSize { expected, found },
        ErrorKind::SampleCount { expected, decoded } => Error::SampleCount { expected, decoded },
        ErrorKind::Md5Mismatch { expected, decoded } => Error::Md5Mismatch {
            expected: hex(&expected),
            decoded: hex(&decoded),
        },
        _ => Error::Decode(error.to_string()),
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}
