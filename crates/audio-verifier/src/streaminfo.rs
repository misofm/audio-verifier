use crate::{Error, SUPPORTED_BIT_DEPTHS, SUPPORTED_SAMPLE_RATES};

const FLAC_MAGIC: [u8; 4] = *b"fLaC";
const STREAMINFO_TYPE: u8 = 0;
const STREAMINFO_LENGTH: u32 = 34;
const HEADER_LENGTH: usize = 8 + STREAMINFO_LENGTH as usize;
const MIN_BLOCK_SIZE_OFFSET: usize = 8;
const MAX_BLOCK_SIZE_OFFSET: usize = 10;
const MIN_FRAME_SIZE_OFFSET: usize = 12;
const MAX_FRAME_SIZE_OFFSET: usize = 15;
const PACKED_OFFSET: usize = 18;
const MD5_OFFSET: usize = 26;
const MIN_BLOCK_SIZE_FLOOR: u16 = 16;

/// STREAMINFO fields of a canonical FLAC file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StreamInfo {
    /// Smallest block size in samples, excluding the last block.
    pub min_block_size: u16,
    /// Largest block size in samples.
    pub max_block_size: u16,
    /// Smallest frame size in bytes, or 0 if unknown.
    pub min_frame_size: u32,
    /// Largest frame size in bytes, or 0 if unknown.
    pub max_frame_size: u32,
    /// Sample rate in hertz.
    pub sample_rate_hz: u32,
    /// Channel count.
    pub channels: u8,
    /// Bits per sample.
    pub bit_depth: u8,
    /// Frames per channel.
    pub total_samples: u64,
    /// MD5 of the canonical PCM.
    pub md5: [u8; 16],
}

impl StreamInfo {
    /// Parses STREAMINFO, requiring it to be the first and only metadata block.
    pub fn parse_canonical(bytes: &[u8]) -> Result<Self, Error> {
        if bytes.len() < HEADER_LENGTH {
            return Err(Error::TooShort(bytes.len()));
        }
        if bytes[..4] != FLAC_MAGIC {
            return Err(Error::MissingMagic);
        }
        let block_header = bytes[4];
        let block_type = block_header & 0x7f;
        if block_type != STREAMINFO_TYPE {
            return Err(Error::WrongFirstBlock(block_type));
        }
        if block_header & 0x80 == 0 {
            return Err(Error::ExtraMetadata);
        }
        let length = u32::from_be_bytes([0, bytes[5], bytes[6], bytes[7]]);
        if length != STREAMINFO_LENGTH {
            return Err(Error::StreamInfoLength(length));
        }

        let packed = u64::from_be_bytes(read_array(bytes, PACKED_OFFSET));
        Ok(Self {
            min_block_size: u16::from_be_bytes(read_array(bytes, MIN_BLOCK_SIZE_OFFSET)),
            max_block_size: u16::from_be_bytes(read_array(bytes, MAX_BLOCK_SIZE_OFFSET)),
            min_frame_size: read_u24(bytes, MIN_FRAME_SIZE_OFFSET),
            max_frame_size: read_u24(bytes, MAX_FRAME_SIZE_OFFSET),
            sample_rate_hz: (packed >> 44) as u32 & 0x0f_ffff,
            channels: ((packed >> 41) & 0x07) as u8 + 1,
            bit_depth: ((packed >> 36) & 0x1f) as u8 + 1,
            total_samples: packed & 0x0f_ffff_ffff,
            md5: read_array(bytes, MD5_OFFSET),
        })
    }

    /// Applies the attestation rules that STREAMINFO alone can decide.
    pub fn validate(&self) -> Result<(), Error> {
        if self.md5 == [0; 16] {
            return Err(Error::ZeroMd5);
        }
        if self.total_samples == 0 {
            return Err(Error::ZeroTotalSamples);
        }
        if self.bit_depth == 32 {
            return Err(Error::Unsupported32Bit);
        }
        if !SUPPORTED_BIT_DEPTHS.contains(&self.bit_depth) {
            return Err(Error::UnsupportedBitDepth(self.bit_depth));
        }
        if !SUPPORTED_SAMPLE_RATES.contains(&self.sample_rate_hz) {
            return Err(Error::UnsupportedSampleRate(self.sample_rate_hz));
        }
        if self.min_block_size < MIN_BLOCK_SIZE_FLOOR || self.min_block_size > self.max_block_size {
            return Err(Error::BlockSizeBounds {
                min: self.min_block_size,
                max: self.max_block_size,
            });
        }
        Ok(())
    }

    /// Bytes per sample in the digested PCM stream, `ceil(bit_depth / 8)`.
    pub fn bytes_per_sample(&self) -> usize {
        usize::from(self.bit_depth.div_ceil(8))
    }
}

fn read_u24(bytes: &[u8], offset: usize) -> u32 {
    let [high, middle, low] = read_array(bytes, offset);
    u32::from_be_bytes([0, high, middle, low])
}

fn read_array<const N: usize>(bytes: &[u8], offset: usize) -> [u8; N] {
    let mut array = [0; N];
    array.copy_from_slice(&bytes[offset..offset + N]);
    array
}
