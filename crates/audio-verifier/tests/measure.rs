mod common;

use audio_verifier::{AudioPayload, Error, blob_id_base64, compute_blob_id};
use common::{
    FrameHeader, Rate, Synth, block_sizes, committed_blob_id, decoded_samples, ffmpeg,
    ffmpeg_raw_pcm, fixture, fixture_path, frame_sizes, pcm_md5, scratch_file, set_block_sizes,
    set_frame_sizes, set_md5, set_streaminfo, walrus_cli, walrus_cli_blob_id,
};

// Optional migration evidence from the same inputs and assertions as this suite.
fn measure(bytes: &[u8]) -> Result<AudioPayload, Error> {
    let result = audio_verifier::measure(bytes);
    if let Some(directory) = std::env::var_os("AUDIO_VERIFIER_MIGRATION_RESULTS") {
        let directory = std::path::PathBuf::from(directory);
        std::fs::create_dir_all(&directory).expect("migration directory");
        let id = blake3::hash(bytes).to_hex().to_string();
        let value = match &result {
            Ok(payload) => serde_json::json!({"accepted": true, "payload": payload}),
            Err(error) => serde_json::json!({"accepted": false, "error": error.to_string()}),
        };
        std::fs::write(directory.join(format!("{id}.flac")), bytes).expect("migration input");
        std::fs::write(
            directory.join(format!("{id}.json")),
            serde_json::to_vec_pretty(&value).expect("migration JSON"),
        )
        .expect("migration result");
    }
    result
}

struct Expected {
    name: &'static str,
    channels: u8,
    bit_depth: u8,
    sample_rate_hz: u32,
    samples_per_channel: u64,
}

const POSITIVE: [Expected; 3] = [
    Expected {
        name: "s16_stereo_44100",
        channels: 2,
        bit_depth: 16,
        sample_rate_hz: 44_100,
        samples_per_channel: 11_025,
    },
    Expected {
        name: "s24_stereo_48000",
        channels: 2,
        bit_depth: 24,
        sample_rate_hz: 48_000,
        samples_per_channel: 12_000,
    },
    Expected {
        name: "s16_mono_96000",
        channels: 1,
        bit_depth: 16,
        sample_rate_hz: 96_000,
        samples_per_channel: 24_000,
    },
];

const ENCODER_OUTPUT: [Expected; 5] = [
    Expected {
        name: "s16_stereo_44100_short",
        channels: 2,
        bit_depth: 16,
        sample_rate_hz: 44_100,
        samples_per_channel: 2_205,
    },
    Expected {
        name: "libflac_s16_stereo_44100",
        channels: 2,
        bit_depth: 16,
        sample_rate_hz: 44_100,
        samples_per_channel: 11_025,
    },
    Expected {
        name: "libflac_s24_stereo_96000",
        channels: 2,
        bit_depth: 24,
        sample_rate_hz: 96_000,
        samples_per_channel: 9_600,
    },
    Expected {
        name: "libflac_s16_mono_48000_short",
        channels: 1,
        bit_depth: 16,
        sample_rate_hz: 48_000,
        samples_per_channel: 10,
    },
    Expected {
        name: "s16_mono_44100",
        channels: 1,
        bit_depth: 16,
        sample_rate_hz: 44_100,
        samples_per_channel: 8_820,
    },
];

const VARIABLE_BLOCKS: [usize; 4] = [1_024, 2_048, 512, 300];

fn flac_name(name: &str) -> String {
    format!("{name}.flac")
}

fn assert_properties(
    audio: &AudioPayload,
    channels: u8,
    bit_depth: u8,
    rate: u32,
    samples_per_channel: u64,
) {
    assert_eq!(audio.format, "flac");
    assert_eq!(audio.channels, channels);
    assert_eq!(audio.bit_depth, bit_depth);
    assert_eq!(audio.sample_rate_hz, rate);
    assert_eq!(audio.samples_per_channel, samples_per_channel);
    assert_eq!(audio.pcm_digest.len(), 32);
}

fn assert_matches_ffmpeg(bytes: &[u8], audio: &AudioPayload, scratch_name: &str) {
    let Some(ffmpeg) = ffmpeg() else {
        eprintln!("ffmpeg unavailable, skipping PCM cross-check for {scratch_name}");
        return;
    };
    let path = scratch_file(scratch_name, bytes);
    let raw = ffmpeg_raw_pcm(&ffmpeg, &path, audio.bit_depth);
    let frame_bytes = u64::from(audio.channels) * u64::from(audio.bit_depth.div_ceil(8));
    assert_eq!(raw.len() as u64, audio.samples_per_channel * frame_bytes);
    assert_eq!(audio.pcm_digest, blake3::hash(&raw).as_bytes().to_vec());
}

fn rejects(bytes: &[u8]) -> Error {
    measure(bytes).expect_err("must be rejected")
}

#[test]
fn ffmpeg_fixtures_measure_correct_properties() {
    for expected in POSITIVE {
        let audio = measure(&fixture(&flac_name(expected.name))).expect(expected.name);
        assert_properties(
            &audio,
            expected.channels,
            expected.bit_depth,
            expected.sample_rate_hz,
            expected.samples_per_channel,
        );
    }
}

#[test]
fn pcm_digest_is_blake3_of_ffmpeg_raw_decode() {
    for expected in POSITIVE {
        let name = flac_name(expected.name);
        let bytes = fixture(&name);
        let audio = measure(&bytes).expect(expected.name);
        assert_matches_ffmpeg(&bytes, &audio, &name);
    }
}

#[test]
fn blob_id_matches_walrus_cli_output() {
    for expected in POSITIVE {
        let name = flac_name(expected.name);
        let audio = measure(&fixture(&name)).expect(expected.name);
        assert_eq!(
            blob_id_base64(&audio.blob_id),
            committed_blob_id(expected.name)
        );
        if let Some(cli) = walrus_cli() {
            assert_eq!(
                blob_id_base64(&audio.blob_id),
                walrus_cli_blob_id(&cli, &fixture_path(&name))
            );
        } else {
            eprintln!("WALRUS_CLI unset, compared {name} against the committed CLI output only");
        }
    }
}

#[test]
fn synthesized_8_bit_stereo_is_measured() {
    let bytes = Synth::new(2, 8, 44_100).build();
    let audio = measure(&bytes).expect("8-bit");
    assert_properties(&audio, 2, 8, 44_100, 2_500);
    assert_matches_ffmpeg(&bytes, &audio, "s8_stereo_44100.flac");
    if let Some(cli) = walrus_cli() {
        let path = scratch_file("s8_stereo_44100.flac", &bytes);
        assert_eq!(
            blob_id_base64(&audio.blob_id),
            walrus_cli_blob_id(&cli, &path)
        );
    }
}

#[test]
fn synthesized_24_bit_mono_is_measured() {
    let bytes = Synth::new(1, 24, 192_000).build();
    let audio = measure(&bytes).expect("24-bit");
    assert_properties(&audio, 1, 24, 192_000, 2_500);
    assert_matches_ffmpeg(&bytes, &audio, "s24_mono_192000.flac");
}

#[test]
fn frame_rate_encodings_equal_to_streaminfo_are_accepted() {
    for rate in [
        Rate::FromStreamInfo,
        Rate::Khz(48),
        Rate::Hz(48_000),
        Rate::TensOfHz(4_800),
    ] {
        let bytes = Synth::new(2, 16, 48_000)
            .with_header(FrameHeader {
                channels: 2,
                bit_depth: 16,
                rate,
            })
            .build();
        let audio = measure(&bytes).unwrap_or_else(|error| panic!("{rate:?}: {error}"));
        assert_eq!(audio.sample_rate_hz, 48_000);
    }
}

#[test]
fn blob_id_is_computed_over_the_whole_file() {
    let bytes = fixture("s16_stereo_44100.flac");
    let audio = measure(&bytes).expect("fixture");
    assert_eq!(audio.blob_id, compute_blob_id(&bytes).expect("blob ID"));
}

#[test]
fn rejects_vorbis_comment_after_streaminfo() {
    let mut bytes = fixture("s16_stereo_44100.flac");
    bytes[4] = 0x00;
    let comment = [0x84, 0, 0, 8, 0, 0, 0, 0, 0, 0, 0, 0];
    bytes.splice(42..42, comment);
    assert_eq!(rejects(&bytes), Error::ExtraMetadata);
}

#[test]
fn rejects_padding_after_streaminfo() {
    let mut bytes = fixture("s16_stereo_44100.flac");
    bytes[4] = 0x00;
    bytes.splice(42..42, [0x81, 0, 0, 4, 0, 0, 0, 0]);
    assert_eq!(rejects(&bytes), Error::ExtraMetadata);
}

#[test]
fn rejects_wrong_first_block() {
    let mut bytes = fixture("s16_stereo_44100.flac");
    bytes[4] = 0x00;
    bytes.splice(4..4, [0x01, 0, 0, 4, 0, 0, 0, 0]);
    assert_eq!(rejects(&bytes), Error::WrongFirstBlock(1));
}

#[test]
fn rejects_wrong_streaminfo_length() {
    let mut bytes = fixture("s16_stereo_44100.flac");
    bytes[7] = 33;
    assert_eq!(rejects(&bytes), Error::StreamInfoLength(33));
    bytes[7] = 35;
    assert_eq!(rejects(&bytes), Error::StreamInfoLength(35));
}

#[test]
fn rejects_zero_md5() {
    let mut bytes = fixture("s16_stereo_44100.flac");
    set_md5(&mut bytes, [0; 16]);
    assert_eq!(rejects(&bytes), Error::ZeroMd5);
}

#[test]
fn rejects_md5_mismatch() {
    let mut bytes = fixture("s16_stereo_44100.flac");
    bytes[30] ^= 0x01;
    assert!(matches!(rejects(&bytes), Error::Md5Mismatch { .. }));
}

#[test]
fn rejects_zero_total_samples() {
    let mut bytes = fixture("s16_stereo_44100.flac");
    set_streaminfo(&mut bytes, 44_100, 2, 16, 0);
    assert_eq!(rejects(&bytes), Error::ZeroTotalSamples);
}

#[test]
fn rejects_total_samples_mismatch() {
    for total in [11_024, 11_026, 22_050] {
        let mut bytes = fixture("s16_stereo_44100.flac");
        set_streaminfo(&mut bytes, 44_100, 2, 16, total);
        assert_eq!(
            rejects(&bytes),
            Error::SampleCount {
                expected: total,
                decoded: 11_025
            }
        );
    }
}

#[test]
fn rejects_32_bit_flac() {
    assert_eq!(
        rejects(&fixture("s32_stereo_48000.flac")),
        Error::Unsupported32Bit
    );
}

#[test]
fn rejects_unsupported_bit_depth() {
    let mut bytes = fixture("s16_stereo_44100.flac");
    set_streaminfo(&mut bytes, 44_100, 2, 12, 11_025);
    assert_eq!(rejects(&bytes), Error::UnsupportedBitDepth(12));
}

#[test]
fn rejects_unsupported_sample_rate() {
    assert_eq!(
        rejects(&fixture("s16_stereo_22050.flac")),
        Error::UnsupportedSampleRate(22_050)
    );
}

#[test]
fn rejects_frame_channels_disagreeing_with_streaminfo() {
    let bytes = Synth::new(2, 16, 44_100)
        .with_header(FrameHeader {
            channels: 1,
            bit_depth: 16,
            rate: Rate::standard(44_100),
        })
        .build();
    assert!(matches!(
        rejects(&bytes),
        Error::FrameChannels {
            frame: 0,
            expected: 2,
            found: 1
        }
    ));
}

#[test]
fn rejects_frame_sample_rate_disagreeing_with_streaminfo() {
    for rate in [
        Rate::standard(44_100),
        Rate::Khz(44),
        Rate::Hz(44_100),
        Rate::TensOfHz(4_410),
    ] {
        let bytes = Synth::new(2, 16, 48_000)
            .with_header(FrameHeader {
                channels: 2,
                bit_depth: 16,
                rate,
            })
            .build();
        assert!(
            matches!(
                rejects(&bytes),
                Error::FrameSampleRate {
                    frame: 0,
                    expected: 48_000,
                    ..
                }
            ),
            "{rate:?}"
        );
    }
}

#[test]
fn rejects_frame_bit_depth_disagreeing_with_streaminfo() {
    let bytes = Synth::new(2, 24, 48_000)
        .with_header(FrameHeader {
            channels: 2,
            bit_depth: 16,
            rate: Rate::standard(48_000),
        })
        .build();
    assert!(matches!(
        rejects(&bytes),
        Error::FrameBitDepth {
            frame: 0,
            expected: 24,
            found: 16
        }
    ));
}

#[test]
fn rejects_mismatch_in_a_later_frame() {
    let mut synth = Synth::new(2, 16, 44_100).with_header(FrameHeader {
        channels: 2,
        bit_depth: 16,
        rate: Rate::standard(96_000),
    });
    synth.override_from_block = 2;
    assert!(matches!(
        rejects(&synth.build()),
        Error::FrameSampleRate {
            frame: 2,
            expected: 44_100,
            found: 96_000
        }
    ));
}

#[test]
fn rejects_mono_file_relabelled_as_stereo_48k() {
    let mut bytes = fixture("s16_mono_44100.flac");
    measure(&bytes).expect("honest mono file is accepted");
    let samples = decoded_samples(&bytes);
    assert_eq!(samples.len(), 8_820);
    set_streaminfo(&mut bytes, 48_000, 2, 16, 4_410);
    set_md5(&mut bytes, pcm_md5(&samples, 2));
    assert!(matches!(
        rejects(&bytes),
        Error::FrameChannels {
            frame: 0,
            expected: 2,
            found: 1
        }
    ));
}

#[test]
fn rejects_ffmpeg_file_with_patched_sample_rate() {
    let mut bytes = fixture("s16_stereo_44100.flac");
    set_streaminfo(&mut bytes, 48_000, 2, 16, 11_025);
    assert!(matches!(
        rejects(&bytes),
        Error::FrameSampleRate {
            expected: 48_000,
            found: 44_100,
            ..
        }
    ));
}

#[test]
fn rejects_ffmpeg_file_with_patched_bit_depth() {
    let mut bytes = fixture("s16_stereo_44100.flac");
    let samples = decoded_samples(&bytes);
    set_streaminfo(&mut bytes, 44_100, 2, 24, 11_025);
    set_md5(&mut bytes, pcm_md5(&samples, 3));
    assert!(matches!(
        rejects(&bytes),
        Error::FrameBitDepth {
            expected: 24,
            found: 16,
            ..
        }
    ));
}

#[test]
fn rejects_truncated_file() {
    let bytes = fixture("s16_stereo_44100.flac");
    for len in [41, 42, 100, bytes.len() / 2, bytes.len() - 1] {
        assert!(measure(&bytes[..len]).is_err(), "length {len}");
    }
}

#[test]
fn rejects_trailing_garbage() {
    let mut bytes = fixture("s16_stereo_44100.flac");
    bytes.extend([0u8; 16]);
    assert!(matches!(rejects(&bytes), Error::Decode(_)));
}

#[test]
fn rejects_empty_input() {
    assert_eq!(rejects(&[]), Error::TooShort(0));
}

#[test]
fn rejects_non_flac_input() {
    assert_eq!(rejects(&[0x42; 64]), Error::MissingMagic);
}

fn spec_pcm_digest(samples: &[i32], bit_depth: u8) -> Vec<u8> {
    let width = u32::from(bit_depth.div_ceil(8));
    let modulus = 1i64 << (8 * width);
    let mut pcm = Vec::new();
    for &sample in samples {
        let value = i64::from(sample).rem_euclid(modulus);
        pcm.extend((0..width).map(|byte| (value >> (8 * byte)) as u8));
    }
    blake3::hash(&pcm).as_bytes().to_vec()
}

#[test]
fn pcm_digest_follows_the_audio_spec_at_every_accepted_bit_depth() {
    let cases = [
        (8, Synth::new(2, 8, 44_100).build()),
        (16, fixture("s16_stereo_44100.flac")),
        (24, fixture("s24_stereo_48000.flac")),
        (24, Synth::new(1, 24, 192_000).build()),
    ];
    for (bit_depth, bytes) in cases {
        let audio = measure(&bytes).expect("accepted");
        assert_eq!(audio.bit_depth, bit_depth);
        let samples = decoded_samples(&bytes);
        let bound = 1i32 << (bit_depth - 1);
        assert!(
            samples
                .iter()
                .all(|&sample| (-bound..bound).contains(&sample))
        );
        assert!(
            samples.iter().any(|&sample| sample < 0),
            "{bit_depth}-bit case has no negative sample to check sign extension"
        );
        assert_eq!(audio.pcm_digest, spec_pcm_digest(&samples, bit_depth));
    }
}

#[test]
fn ffmpeg_and_libflac_output_claims_exact_block_and_frame_sizes() {
    for expected in POSITIVE.iter().chain(&ENCODER_OUTPUT) {
        let name = flac_name(expected.name);
        let bytes = fixture(&name);
        let (min_frame, max_frame) = frame_sizes(&bytes);
        assert!(
            min_frame > 0 && max_frame > 0,
            "{name} leaves frame sizes unknown"
        );
        let audio = measure(&bytes).unwrap_or_else(|error| panic!("{name}: {error}"));
        assert_properties(
            &audio,
            expected.channels,
            expected.bit_depth,
            expected.sample_rate_hz,
            expected.samples_per_channel,
        );
        assert_matches_ffmpeg(&bytes, &audio, &name);
    }
}

#[test]
fn single_frame_may_be_shorter_than_both_block_sizes() {
    for (name, block_size) in [
        ("s16_stereo_44100_short.flac", 4_608),
        ("libflac_s16_mono_48000_short.flac", 4_096),
    ] {
        let bytes = fixture(name);
        assert_eq!(block_sizes(&bytes), (block_size, block_size), "{name}");
        measure(&bytes).unwrap_or_else(|error| panic!("{name}: {error}"));
    }
}

#[test]
fn variable_block_sizes_with_a_short_last_block_are_accepted() {
    for blocks in [&VARIABLE_BLOCKS[..], &[64, 10]] {
        let bytes = Synth::new(2, 16, 48_000)
            .with_variable_block_sizes(blocks)
            .build();
        let audio = measure(&bytes).unwrap_or_else(|error| panic!("{blocks:?}: {error}"));
        assert_eq!(
            audio.samples_per_channel,
            blocks.iter().sum::<usize>() as u64
        );
        assert_matches_ffmpeg(&bytes, &audio, "variable_blocks.flac");
    }
}

#[test]
fn unknown_frame_sizes_are_accepted() {
    let honest = fixture("s16_stereo_44100.flac");
    let expected = measure(&honest).expect("fixture");
    let (min, max) = frame_sizes(&honest);
    for (patched_min, patched_max) in [(0, max), (min, 0), (0, 0)] {
        let mut bytes = honest.clone();
        set_frame_sizes(&mut bytes, patched_min, patched_max);
        let audio = measure(&bytes).expect("unknown frame size");
        assert_eq!(audio.pcm_digest, expected.pcm_digest);
    }
}

#[test]
fn rejects_block_sizes_outside_the_streaminfo_bounds() {
    for (min, max) in [(0, 4_608), (15, 4_608), (4_609, 4_608), (16, 15), (0, 0)] {
        let mut bytes = fixture("s16_stereo_44100.flac");
        set_block_sizes(&mut bytes, min, max);
        assert_eq!(
            rejects(&bytes),
            Error::BlockSizeBounds { min, max },
            "{min} to {max}"
        );
    }
}

#[test]
fn rejects_min_block_size_too_low() {
    let mut bytes = fixture("s16_stereo_44100.flac");
    set_block_sizes(&mut bytes, 4_000, 4_608);
    assert_eq!(
        rejects(&bytes),
        Error::MinBlockSize {
            expected: 4_000,
            found: 4_608
        }
    );

    let mut bytes = Synth::new(2, 16, 48_000)
        .with_variable_block_sizes(&VARIABLE_BLOCKS)
        .build();
    set_block_sizes(&mut bytes, 511, 2_048);
    assert_eq!(
        rejects(&bytes),
        Error::MinBlockSize {
            expected: 511,
            found: 512
        }
    );
}

#[test]
fn rejects_min_block_size_too_high() {
    let mut bytes = Synth::new(2, 16, 48_000)
        .with_variable_block_sizes(&VARIABLE_BLOCKS)
        .build();
    set_block_sizes(&mut bytes, 513, 2_048);
    assert_eq!(
        rejects(&bytes),
        Error::FrameBlockSizeBelowMin {
            frame: 2,
            expected: 513,
            found: 512
        }
    );

    let mut bytes = fixture("libflac_s16_stereo_44100.flac");
    set_block_sizes(&mut bytes, 4_097, 4_097);
    assert_eq!(
        rejects(&bytes),
        Error::FrameBlockSizeBelowMin {
            frame: 0,
            expected: 4_097,
            found: 4_096
        }
    );
}

#[test]
fn rejects_max_block_size_too_low() {
    let mut bytes = Synth::new(2, 16, 48_000)
        .with_variable_block_sizes(&VARIABLE_BLOCKS)
        .build();
    set_block_sizes(&mut bytes, 512, 2_047);
    assert_eq!(
        rejects(&bytes),
        Error::FrameBlockSizeAboveMax {
            frame: 1,
            expected: 2_047,
            found: 2_048
        }
    );

    let mut bytes = fixture("s16_stereo_44100_short.flac");
    set_block_sizes(&mut bytes, 2_204, 2_204);
    assert_eq!(
        rejects(&bytes),
        Error::FrameBlockSizeAboveMax {
            frame: 0,
            expected: 2_204,
            found: 2_205
        }
    );
}

#[test]
fn rejects_max_block_size_too_high() {
    let mut bytes = fixture("s16_stereo_44100.flac");
    set_block_sizes(&mut bytes, 4_608, 4_609);
    assert_eq!(
        rejects(&bytes),
        Error::MaxBlockSize {
            expected: 4_609,
            found: 4_608
        }
    );

    let mut bytes = Synth::new(2, 16, 48_000)
        .with_variable_block_sizes(&VARIABLE_BLOCKS)
        .build();
    set_block_sizes(&mut bytes, 512, 2_049);
    assert_eq!(
        rejects(&bytes),
        Error::MaxBlockSize {
            expected: 2_049,
            found: 2_048
        }
    );
}

#[test]
fn rejects_wrong_min_frame_size() {
    for name in ["s16_stereo_44100.flac", "libflac_s24_stereo_96000.flac"] {
        let honest = fixture(name);
        let (min, max) = frame_sizes(&honest);
        for claimed in [1, min - 1, min + 1] {
            let mut bytes = honest.clone();
            set_frame_sizes(&mut bytes, claimed, max);
            assert_eq!(
                rejects(&bytes),
                Error::MinFrameSize {
                    expected: claimed,
                    found: u64::from(min)
                },
                "{name} claiming {claimed}"
            );
        }
    }
}

#[test]
fn rejects_wrong_max_frame_size() {
    for name in ["s16_stereo_44100.flac", "libflac_s24_stereo_96000.flac"] {
        let honest = fixture(name);
        let (min, max) = frame_sizes(&honest);
        for claimed in [max - 1, max + 1, 0x00ff_ffff] {
            let mut bytes = honest.clone();
            set_frame_sizes(&mut bytes, min, claimed);
            assert_eq!(
                rejects(&bytes),
                Error::MaxFrameSize {
                    expected: claimed,
                    found: u64::from(max)
                },
                "{name} claiming {claimed}"
            );
        }
    }
}
