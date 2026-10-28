use std::time::Duration;

use audio_verifier::{
    AudioEncodingPayload, AudioPayload, StreamInfo, blob_id_base64, measure, u256_le_decimal,
};
use axum::{Router, body::Bytes};
use nautilus::{ConfigError, Intent, NautilusContext, NautilusError, ServerConfig, SignedResponse};
use serde::{Deserialize, Serialize};

/// Intent of the signed `IntentMessage<AudioPayload>`.
pub const AUDIO_INTENT: Intent = Intent(0);

/// Intent of the signed `IntentMessage<AudioEncodingPayload>`.
pub const AUDIO_ENCODING_INTENT: Intent = Intent(1);

/// Default `/process_data` body limit.
pub const DEFAULT_MAX_BODY_BYTES: usize = 256 * 1024 * 1024;

/// Maximum decoded PCM work per request, including all channels (4 GiB).
/// Compressed uploads can otherwise expand to terabytes of hashing work.
pub const MAX_DECODED_BYTES: u64 = 4 * 1024 * 1024 * 1024;

/// Deadline for one upload and measurement.
pub const REQUEST_TIMEOUT: Duration = Duration::from_secs(120);

/// Deadline for the upload alone: 256 MiB needs about 3 MB/s, leaving at least 30 s to measure.
pub const BODY_READ_TIMEOUT: Duration = Duration::from_secs(90);

/// Open connections, matching the host bridge's 256. Idle ones cost little, because the single
/// `max_concurrent` permit is taken before a body is read; a low cap let a few idle connections
/// lock out every request, `/health_check` included, until `HEADER_READ_TIMEOUT` reaped them.
pub const MAX_CONNECTIONS: usize = 256;

/// Deadline for request headers, including a kept-alive connection's next request.
pub const HEADER_READ_TIMEOUT: Duration = Duration::from_secs(10);

/// Closes a connection that does no I/O for this long outside a handler.
pub const IDLE_TIMEOUT: Duration = Duration::from_secs(30);

/// Environment variable that overrides the body limit.
pub const MAX_BODY_BYTES_ENV: &str = "AUDIO_VERIFIER_MAX_BODY_BYTES";

/// JSON view of an `AudioPayload`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AudioJson {
    /// Channel count.
    pub channels: u8,
    /// Bits per sample.
    pub bit_depth: u8,
    /// Sample rate in hertz.
    pub sample_rate_hz: u32,
    /// Samples per channel as a decimal string.
    pub samples_per_channel: String,
    /// BLAKE3 PCM digest as hex.
    pub pcm_digest: String,
}

impl From<&AudioPayload> for AudioJson {
    fn from(audio: &AudioPayload) -> Self {
        Self {
            channels: audio.channels,
            bit_depth: audio.bit_depth,
            sample_rate_hz: audio.sample_rate_hz,
            samples_per_channel: audio.samples_per_channel.to_string(),
            pcm_digest: hex::encode(&audio.pcm_digest),
        }
    }
}

/// JSON view of an `AudioEncodingPayload` beyond the audio facts, which are in [`AudioJson`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AudioEncodingJson {
    /// Walrus base64url blob ID.
    pub blob_id: String,
    /// Blob ID as the decimal Move `u256`.
    pub blob_id_u256: String,
}

impl From<&AudioEncodingPayload> for AudioEncodingJson {
    fn from(encoding: &AudioEncodingPayload) -> Self {
        Self {
            blob_id: blob_id_base64(&encoding.blob_id),
            blob_id_u256: u256_le_decimal(&encoding.blob_id),
        }
    }
}

/// One signed payload in the `/process_data` response.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SignedJson<T> {
    /// Intent byte.
    pub intent: u8,
    /// Signing time in milliseconds since the Unix epoch, from the enclave clock.
    pub timestamp_ms: u64,
    /// The payload's fields.
    #[serde(flatten)]
    pub data: T,
    /// Ed25519 signature over the BCS intent message, as hex.
    pub signature: String,
}

impl<T> SignedJson<T> {
    fn new<P>(signed: &SignedResponse<P>, data: T) -> Self {
        Self {
            intent: signed.response.intent.0,
            timestamp_ms: signed.response.timestamp_ms,
            data,
            signature: signed.signature.clone(),
        }
    }
}

/// `/process_data` response body: the audio and the file, signed separately.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProcessDataResponse {
    /// The audio facts, for `verify_audio`.
    pub audio: SignedJson<AudioJson>,
    /// The file's blob ID, for `verify_encoding`; the signed facts are those in `audio`.
    pub audio_encoding: SignedJson<AudioEncodingJson>,
    /// Enclave Ed25519 public key as hex.
    pub public_key: String,
}

/// Renders the two signed payloads as the `/process_data` response.
pub fn response(
    ctx: &NautilusContext,
    audio: SignedResponse<AudioPayload>,
    encoding: SignedResponse<AudioEncodingPayload>,
) -> ProcessDataResponse {
    ProcessDataResponse {
        audio: SignedJson::new(&audio, AudioJson::from(&audio.response.data)),
        audio_encoding: SignedJson::new(
            &encoding,
            AudioEncodingJson::from(&encoding.response.data),
        ),
        public_key: ctx.public_key_hex(),
    }
}

/// Measures `bytes` once and signs the audio and the encoding.
pub fn process(bytes: &[u8], ctx: &NautilusContext) -> Result<ProcessDataResponse, NautilusError> {
    let info = StreamInfo::parse_canonical(bytes)
        .and_then(|info| info.validate().map(|()| info))
        .map_err(|error| NautilusError::application(error.to_string()))?;
    let decoded_bytes = info
        .total_samples
        .checked_mul(u64::from(info.channels))
        .and_then(|samples| samples.checked_mul(info.bytes_per_sample() as u64));
    if decoded_bytes.is_none_or(|size| size > MAX_DECODED_BYTES) {
        return Err(NautilusError::application(
            "decoded PCM exceeds the 4 GiB service limit",
        ));
    }
    let encoding = measure(bytes).map_err(|error| NautilusError::application(error.to_string()))?;
    let audio = ctx.sign_intent(AUDIO_INTENT, encoding.audio())?;
    let encoding = ctx.sign_intent(AUDIO_ENCODING_INTENT, encoding)?;
    Ok(response(ctx, audio, encoding))
}

/// Server limits for one request at a time; pass the same value to the router and the listener.
pub fn server_config(max_body_bytes: usize) -> ServerConfig {
    ServerConfig {
        max_body_bytes,
        max_concurrent: 1,
        request_timeout: REQUEST_TIMEOUT,
        body_read_timeout: BODY_READ_TIMEOUT,
        max_connections: MAX_CONNECTIONS,
        header_read_timeout: HEADER_READ_TIMEOUT,
        idle_timeout: IDLE_TIMEOUT,
        allow_unnonced_attestation: false,
    }
}

/// Builds the enclave router around an injected context.
pub fn app(ctx: NautilusContext, config: ServerConfig) -> Result<Router, ConfigError> {
    nautilus::router(ctx, config, |bytes: Bytes, ctx: &NautilusContext| {
        process(&bytes, ctx)
    })
}

/// Resolves the body limit from `--max-body-bytes`, then the environment, then the default.
pub fn max_body_bytes(args: &[String], env: Option<String>) -> anyhow::Result<usize> {
    let mut flag = None;
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--max-body-bytes" => {
                let value = iter
                    .next()
                    .ok_or_else(|| anyhow::anyhow!("--max-body-bytes needs a value"))?;
                anyhow::ensure!(
                    flag.is_none(),
                    "--max-body-bytes may only be specified once"
                );
                flag = Some(value.clone());
            }
            other => anyhow::bail!("unknown argument {other}"),
        }
    }
    match flag.or(env) {
        Some(value) => {
            let limit = value.trim().parse::<usize>()?;
            anyhow::ensure!(limit > 0, "max body bytes must be greater than zero");
            Ok(limit)
        }
        None => Ok(DEFAULT_MAX_BODY_BYTES),
    }
}
