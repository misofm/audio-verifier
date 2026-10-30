mod decode;
mod error;
mod frame_header;
mod self_test;
mod streaminfo;

use serde::{Deserialize, Serialize};

pub use error::Error;
pub use self_test::self_test;
pub use streaminfo::StreamInfo;

/// Bit depths the enclave attests.
pub const SUPPORTED_BIT_DEPTHS: [u8; 3] = [8, 16, 24];

/// Sample rates the enclave attests: 44.1 and 48 kHz and their doublings.
pub const SUPPORTED_SAMPLE_RATES: [u32; 8] = [
    44_100, 48_000, 88_200, 96_000, 176_400, 192_000, 352_800, 384_000,
];

/// BCS mirror of the Move `audio_verifier::audio_verifier::AudioPayload`, field for field:
/// the facts of the decoded audio, signed under the audio intent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AudioPayload {
    /// Channel count.
    pub channels: u8,
    /// Bits per sample.
    pub bit_depth: u8,
    /// Sample rate in hertz.
    pub sample_rate_hz: u32,
    /// Samples per channel.
    pub samples_per_channel: u64,
    /// BLAKE3 of the decoded PCM, interleaved by sample frame, each sample
    /// signed little-endian in `ceil(bit_depth / 8)` bytes.
    pub pcm_digest: Vec<u8>,
}

/// Measures a canonical FLAC file into its Move-layout `AudioPayload`.
pub fn measure(bytes: &[u8]) -> Result<AudioPayload, Error> {
    let info = StreamInfo::parse_canonical(bytes)?;
    info.validate()?;
    let pcm_digest = decode::verify_and_digest(bytes, &info)?;
    Ok(AudioPayload {
        channels: info.channels,
        bit_depth: info.bit_depth,
        sample_rate_hz: info.sample_rate_hz,
        samples_per_channel: info.total_samples,
        pcm_digest: pcm_digest.to_vec(),
    })
}
