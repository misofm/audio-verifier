use std::path::Path;

use audio_verifier::AudioVerificationPayload;
use audio_verifier_enclave::{AUDIO_INTENT, AudioJson, response};
use nautilus::{
    EnclaveIdentity, NautilusContext, intent_message, testing::FakeAttestor, verify_signed_response,
};
use serde::Serialize;

const TIMESTAMP_MS: u64 = 1_750_000_000_000;

#[derive(Serialize)]
struct Vector {
    intent: u8,
    timestamp_ms: u64,
    audio: AudioJson,
    blob_id_hex: String,
    payload_bcs_hex: String,
    intent_message_hex: String,
    public_key: String,
    signature: String,
}

fn fixed_audio() -> AudioVerificationPayload {
    let mut blob_id = [0u8; 32];
    for (index, byte) in blob_id.iter_mut().enumerate() {
        *byte = index as u8;
    }
    AudioVerificationPayload {
        format: "flac".to_owned(),
        channels: 2,
        bit_depth: 24,
        sample_rate_hz: 48_000,
        samples_per_channel: 480_000,
        pcm_digest: vec![0x11; 32],
        blob_id,
    }
}

fn vector() -> (Vector, NautilusContext) {
    let ctx = NautilusContext::with_signer(
        EnclaveIdentity::from_seed([7u8; 32]),
        FakeAttestor::default(),
    );
    let audio = fixed_audio();
    let payload = bcs::to_bytes(&audio).expect("BCS");
    let message = intent_message(AUDIO_INTENT, TIMESTAMP_MS, &audio).expect("BCS");
    let signed = ctx
        .sign_intent_at(AUDIO_INTENT, TIMESTAMP_MS, audio.clone())
        .expect("sign");
    verify_signed_response(&ctx.public_key(), AUDIO_INTENT, &signed).expect("signature verifies");
    let signed = response(&ctx, signed);
    let vector = Vector {
        intent: signed.intent,
        timestamp_ms: signed.timestamp_ms,
        audio: signed.audio,
        blob_id_hex: hex::encode(audio.blob_id),
        payload_bcs_hex: hex::encode(&payload),
        intent_message_hex: hex::encode(&message),
        public_key: signed.public_key,
        signature: signed.signature,
    };
    (vector, ctx)
}

#[test]
fn payload_bcs_matches_move_audio_layout() {
    let payload = bcs::to_bytes(&fixed_audio()).expect("BCS");
    let mut expected = vec![4];
    expected.extend(b"flac");
    expected.extend([2, 24]);
    expected.extend(48_000u32.to_le_bytes());
    expected.extend(480_000u64.to_le_bytes());
    expected.push(32);
    expected.extend([0x11; 32]);
    expected.extend(0u8..32);
    assert_eq!(payload, expected);
    assert_eq!(payload.len(), 5 + 1 + 1 + 4 + 8 + 33 + 32);
}

#[test]
fn vector_signature_verifies() {
    let (vector, ctx) = vector();
    let message = hex::decode(&vector.intent_message_hex).expect("hex");
    assert_eq!(message[0], AUDIO_INTENT.0);
    assert_eq!(&message[1..9], &TIMESTAMP_MS.to_le_bytes());
    assert_eq!(
        hex::encode(&message[9..]),
        vector.payload_bcs_hex,
        "intent message is intent || timestamp || payload"
    );
    let signature: [u8; 64] = hex::decode(&vector.signature)
        .expect("hex")
        .try_into()
        .expect("64 bytes");
    let key = ed25519_dalek::VerifyingKey::from_bytes(&ctx.public_key()).expect("key");
    key.verify_strict(&message, &ed25519_dalek::Signature::from_bytes(&signature))
        .expect("signature verifies");
    assert_eq!(
        vector.audio.blob_id_u256,
        "14074904626401341155369551180448584754667373453244490859944217516317499064576"
    );
}

#[test]
fn committed_vector_is_up_to_date() {
    let (vector, _) = vector();
    let rendered = serde_json::to_string_pretty(&vector).expect("JSON") + "\n";
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../vectors/audio_intent.json");
    let committed = std::fs::read_to_string(&path).ok();
    if committed.as_deref() != Some(rendered.as_str()) {
        std::fs::write(&path, &rendered).expect("write vector");
        panic!(
            "vectors/audio_intent.json was stale and has been regenerated; review and commit it"
        );
    }
}
