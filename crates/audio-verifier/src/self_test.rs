use crate::{Error, StreamInfo, decode};

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

/// Checks the decoder and the PCM digest against recorded answers in a few milliseconds, so that
/// a build that computes wrongly on this CPU can refuse to start.
pub fn self_test() -> Result<(), Error> {
    for (name, flac, expected) in KNOWN_PCM_DIGESTS {
        let info = StreamInfo::parse_canonical(flac)?;
        info.validate()?;
        let digest = decode::verify_and_digest(flac, &info)?;
        if blake3::Hash::from(digest).to_hex().as_str() != expected {
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
