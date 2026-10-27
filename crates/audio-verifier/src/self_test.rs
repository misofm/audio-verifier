use std::num::NonZeroU16;

use crate::{Error, StreamInfo, decode, walrus};

/// 4 by 7 source symbols, against 334 by 667 for mainnet's 1,000 shards.
const N_SHARDS: NonZeroU16 = NonZeroU16::new(10).unwrap();

const KNOWN_PCM_DIGESTS: [(&str, &[u8], &str); 2] = [
    (
        "s16_stereo_44100_short.flac PCM digest",
        include_bytes!("../tests/fixtures/s16_stereo_44100_short.flac"),
        "b32dc75c20dfa641365140e24f286118b2f36d20cd1acf54cb4477efe9e82d7d",
    ),
    (
        "libflac_s24_stereo_96000.flac PCM digest",
        include_bytes!("../tests/fixtures/libflac_s24_stereo_96000.flac"),
        "952893fa9dc608fa02e9f81eb007fe87128d17bab788c3bae09b9dfb79d3aa61",
    ),
];

/// Lengths of generated blobs and their blob IDs at 10 shards from walrus-core c377852. The
/// symbols are 2, 36 and 3,572 bytes.
const KNOWN_BLOB_IDS: [(&str, u32, &str); 3] = [
    (
        "empty blob ID",
        0,
        "gLzpD29smKZiYJIWwKatudKr76kEprQeMPyAhuMbWd4",
    ),
    (
        "1,000-byte blob ID",
        1_000,
        "U_ABxpx5jsiW90E2xzd6raYO3zE4DwOHpLMW7TEvFkw",
    ),
    (
        "100,003-byte blob ID",
        100_003,
        "3EtUZiuXqtxY4lEmQSML3NECRY525-vwitidkz83mes",
    ),
];

/// Checks the decoder, the PCM digest and the blob ID code against recorded answers in a few
/// milliseconds, so that a build that computes wrongly on this CPU can refuse to start.
///
/// Blob IDs are computed for 10 shards, which runs the same BLAKE2b and Reed-Solomon kernels as
/// mainnet's 1,000. One mainnet blob ID takes about 0.1 s, so the 1,000-shard layout is left to
/// the tests against the committed walrus CLI output.
pub fn self_test() -> Result<(), Error> {
    for (name, flac, expected) in KNOWN_PCM_DIGESTS {
        let info = StreamInfo::parse_canonical(flac)?;
        info.validate()?;
        let digest = decode::verify_and_digest(flac, &info)?;
        if blake3::Hash::from(digest).to_hex().as_str() != expected {
            return Err(Error::SelfTest(name));
        }
    }
    let encoder = walrus::blob_encoder(N_SHARDS)?;
    for (name, len, expected) in KNOWN_BLOB_IDS {
        let blob: Vec<u8> = (0..len)
            .map(|index| (index.wrapping_mul(2_654_435_761) >> 24) as u8)
            .collect();
        let blob_id = encoder.blob_id(&blob).map_err(walrus::blob_id_error)?;
        if blob_id.to_string() != expected {
            return Err(Error::SelfTest(name));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn self_test_passes() {
        assert_eq!(self_test(), Ok(()));
    }
}
