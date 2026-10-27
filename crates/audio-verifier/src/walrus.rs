use std::{
    num::NonZeroU16,
    sync::{Arc, OnceLock},
};

use rayon::{ThreadPool, ThreadPoolBuilder};
use walrus_blob_encoder::{BlobEncoder, BlobId, MAINNET_N_SHARDS};

use crate::Error;

/// Walrus mainnet shard count used for blob IDs.
pub const WALRUS_N_SHARDS: NonZeroU16 = MAINNET_N_SHARDS;

/// Leaves two of the enclave's four vCPUs to the decoder and hasher threads of `measure`.
const BLOB_ID_THREADS: usize = 2;

fn pool() -> Arc<ThreadPool> {
    static POOL: OnceLock<Arc<ThreadPool>> = OnceLock::new();
    POOL.get_or_init(|| {
        let pool = ThreadPoolBuilder::new()
            .num_threads(BLOB_ID_THREADS)
            .thread_name(|index| format!("walrus-blob-id-{index}"))
            .build()
            .expect("blob ID thread pool");
        Arc::new(pool)
    })
    .clone()
}

pub(crate) fn blob_encoder(n_shards: NonZeroU16) -> Result<BlobEncoder, Error> {
    BlobEncoder::new(n_shards)
        .map(|encoder| encoder.with_thread_pool(pool()))
        .map_err(blob_id_error)
}

pub(crate) fn blob_id_error(error: walrus_blob_encoder::Error) -> Error {
    Error::BlobId(error.to_string())
}

fn encoder() -> &'static BlobEncoder {
    static ENCODER: OnceLock<BlobEncoder> = OnceLock::new();
    ENCODER.get_or_init(|| blob_encoder(WALRUS_N_SHARDS).expect("mainnet shard count is valid"))
}

/// Computes the Walrus blob ID of `bytes` with the default encoding.
pub fn compute_blob_id(bytes: &[u8]) -> Result<[u8; 32], Error> {
    encoder()
        .blob_id(bytes)
        .map(<[u8; 32]>::from)
        .map_err(blob_id_error)
}

/// Formats blob ID bytes as the Walrus base64url string.
pub fn blob_id_base64(blob_id: &[u8; 32]) -> String {
    BlobId::from(*blob_id).to_string()
}

/// Formats 32 little-endian bytes as a decimal `u256`.
pub fn u256_le_decimal(bytes: &[u8; 32]) -> String {
    let mut limbs: Vec<u8> = bytes.iter().rev().copied().collect();
    let mut digits = Vec::new();
    while limbs.iter().any(|limb| *limb != 0) {
        let mut remainder = 0u16;
        for limb in &mut limbs {
            let value = (remainder << 8) | u16::from(*limb);
            *limb = (value / 10) as u8;
            remainder = value % 10;
        }
        digits.push(b'0' + remainder as u8);
    }
    if digits.is_empty() {
        digits.push(b'0');
    }
    digits.reverse();
    String::from_utf8(digits).expect("ASCII digits")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn u256_decimal_matches_known_values() {
        assert_eq!(u256_le_decimal(&[0; 32]), "0");
        let mut one = [0; 32];
        one[0] = 1;
        assert_eq!(u256_le_decimal(&one), "1");
        let mut word = [0; 32];
        word[..8].copy_from_slice(&u64::MAX.to_le_bytes());
        assert_eq!(u256_le_decimal(&word), u64::MAX.to_string());
        assert_eq!(
            u256_le_decimal(&[0xff; 32]),
            "115792089237316195423570985008687907853269984665640564039457584007913129639935"
        );
        let mut high = [0; 32];
        high[31] = 1;
        assert_eq!(
            u256_le_decimal(&high),
            "452312848583266388373324160190187140051835877600158453279131187530910662656"
        );
    }
}
