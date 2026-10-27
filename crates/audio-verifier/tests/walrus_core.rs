mod common;

use audio_verifier::{WALRUS_N_SHARDS, compute_blob_id};
use common::fixture_path;
use walrus_core::{
    DEFAULT_ENCODING,
    encoding::{EncodingConfig, EncodingFactory as _},
};

/// Source symbols per blob at 1,000 shards: 334 primary by 667 secondary.
const SOURCE_SYMBOLS: usize = 334 * 667;

fn assert_matches_walrus_core(bytes: &[u8]) {
    let expected = EncodingConfig::new(WALRUS_N_SHARDS)
        .get_for_type(DEFAULT_ENCODING)
        .compute_blob_id(bytes)
        .expect("walrus-core blob ID");
    assert_eq!(
        compute_blob_id(bytes).expect("blob ID"),
        expected.0,
        "{} bytes",
        bytes.len()
    );
}

fn pseudo_random(len: usize) -> Vec<u8> {
    let mut state = 0x9e37_79b9_7f4a_7c15_u64 ^ len as u64;
    (0..len)
        .map(|_| {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state as u8
        })
        .collect()
}

#[test]
fn fixtures_match_walrus_core() {
    let fixtures = std::fs::read_dir(fixture_path("")).expect("fixtures directory");
    for entry in fixtures {
        let path = entry.expect("fixture entry").path();
        assert_matches_walrus_core(&std::fs::read(&path).expect("fixture"));
    }
}

#[test]
fn generated_sizes_match_walrus_core() {
    let boundaries = (1..=4).flat_map(|k| {
        let edge = 2 * k * SOURCE_SYMBOLS;
        [edge - 1, edge, edge + 1]
    });
    for len in [0, 1, 2, 3, SOURCE_SYMBOLS, SOURCE_SYMBOLS + 1]
        .into_iter()
        .chain(boundaries)
        .chain([3_000_000, 5 * 1024 * 1024 + 7])
    {
        assert_matches_walrus_core(&pseudo_random(len));
    }
}
