use thiserror::Error;

/// Reasons a file is not attested.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum Error {
    /// Input shorter than the FLAC magic plus a STREAMINFO block.
    #[error("input too short for a FLAC header: {0} bytes")]
    TooShort(usize),
    /// Input does not start with `fLaC`.
    #[error("missing fLaC magic")]
    MissingMagic,
    /// First metadata block is not STREAMINFO.
    #[error("first metadata block has type {0}, expected STREAMINFO")]
    WrongFirstBlock(u8),
    /// STREAMINFO is followed by other metadata blocks.
    #[error("STREAMINFO must be the only metadata block")]
    ExtraMetadata,
    /// STREAMINFO block length is not 34.
    #[error("STREAMINFO length is {0}, expected 34")]
    StreamInfoLength(u32),
    /// STREAMINFO MD5 is all zero.
    #[error("STREAMINFO MD5 is zero")]
    ZeroMd5,
    /// STREAMINFO total samples is zero.
    #[error("STREAMINFO total samples is zero")]
    ZeroTotalSamples,
    /// 32-bit FLAC is excluded by the attestation policy.
    #[error("32-bit FLAC is not supported")]
    Unsupported32Bit,
    /// Bit depth outside 8, 16 and 24.
    #[error("unsupported bit depth {0}")]
    UnsupportedBitDepth(u8),
    /// Sample rate outside the on-chain list.
    #[error("unsupported sample rate {0} Hz")]
    UnsupportedSampleRate(u32),
    /// STREAMINFO block sizes break `16 <= min_block_size <= max_block_size`.
    #[error("STREAMINFO block sizes {min} to {max} are not within 16 to 65535 in order")]
    BlockSizeBounds { min: u16, max: u16 },
    /// Decoder error.
    #[error("FLAC decode failed: {0}")]
    Decode(String),
    /// Decoder STREAMINFO differs from the parsed one.
    #[error("decoder STREAMINFO disagrees with the parsed STREAMINFO")]
    StreamInfoMismatch,
    /// Frame header channel count differs from STREAMINFO.
    #[error("frame {frame} has {found} channels, STREAMINFO says {expected}")]
    FrameChannels {
        frame: u64,
        expected: u8,
        found: u32,
    },
    /// Frame header sample rate differs from STREAMINFO.
    #[error("frame {frame} has sample rate {found} Hz, STREAMINFO says {expected} Hz")]
    FrameSampleRate {
        frame: u64,
        expected: u32,
        found: u32,
    },
    /// Frame header bits per sample differs from STREAMINFO.
    #[error("frame {frame} has {found} bits per sample, STREAMINFO says {expected}")]
    FrameBitDepth {
        frame: u64,
        expected: u8,
        found: u32,
    },
    /// A frame before the last is shorter than the STREAMINFO minimum block size.
    #[error("frame {frame} has {found} samples, STREAMINFO minimum block size is {expected}")]
    FrameBlockSizeBelowMin {
        frame: u64,
        expected: u16,
        found: u32,
    },
    /// A frame is longer than the STREAMINFO maximum block size.
    #[error("frame {frame} has {found} samples, STREAMINFO maximum block size is {expected}")]
    FrameBlockSizeAboveMax {
        frame: u64,
        expected: u16,
        found: u32,
    },
    /// STREAMINFO minimum block size differs from the smallest block before the last.
    #[error("smallest block before the last has {found} samples, STREAMINFO says {expected}")]
    MinBlockSize { expected: u16, found: u32 },
    /// STREAMINFO maximum block size differs from the largest block.
    #[error("largest block has {found} samples, STREAMINFO says {expected}")]
    MaxBlockSize { expected: u16, found: u32 },
    /// Non-zero STREAMINFO minimum frame size differs from the smallest frame.
    #[error("smallest frame has {found} bytes, STREAMINFO says {expected}")]
    MinFrameSize { expected: u32, found: u64 },
    /// Non-zero STREAMINFO maximum frame size differs from the largest frame.
    #[error("largest frame has {found} bytes, STREAMINFO says {expected}")]
    MaxFrameSize { expected: u32, found: u64 },
    /// Decoded frame count differs from STREAMINFO total samples.
    #[error("decoded {decoded} frames, STREAMINFO says {expected}")]
    SampleCount { expected: u64, decoded: u64 },
    /// Decoded PCM MD5 differs from STREAMINFO.
    #[error("PCM MD5 mismatch: STREAMINFO {expected}, decoded {decoded}")]
    Md5Mismatch { expected: String, decoded: String },
    /// A boot self-test computed a wrong answer.
    #[error("self-test failed: {0}")]
    SelfTest(&'static str),
}
