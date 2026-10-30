use std::path::Path;

use audio_verifier::AudioPayload;
use audio_verifier_enclave::{AUDIO_INTENT, AudioJson, ProcessDataResponse, response};
use nautilus::{
    EnclaveIdentity, Intent, NautilusContext, intent_message, testing::FakeAttestor,
    verify_signed_response,
};
use serde::Serialize;

const TIMESTAMP_MS: u64 = 1_750_000_000_000;

const OTHER_INTENT: Intent = Intent(1);

#[derive(Serialize)]
struct Vector<T> {
    intent: u8,
    timestamp_ms: u64,
    fields: T,
    payload_bcs_hex: String,
    intent_message_hex: String,
    public_key: String,
    signature: String,
    other_intent_signature: String,
}

fn fixed_audio() -> AudioPayload {
    AudioPayload {
        channels: 2,
        bit_depth: 24,
        sample_rate_hz: 48_000,
        samples_per_channel: 480_000,
        pcm_digest: vec![0x11; 32],
    }
}

fn context() -> NautilusContext {
    NautilusContext::with_signer(
        EnclaveIdentity::from_seed([7u8; 32]),
        FakeAttestor::default(),
    )
}

fn signed(ctx: &NautilusContext) -> ProcessDataResponse {
    let audio = ctx
        .sign_intent_at(AUDIO_INTENT, TIMESTAMP_MS, fixed_audio())
        .expect("sign");
    verify_signed_response(&ctx.public_key(), AUDIO_INTENT, &audio).expect("audio verifies");
    response(ctx, audio)
}

fn vector<P: Serialize + Clone, T>(
    ctx: &NautilusContext,
    intent: Intent,
    other_intent: Intent,
    payload: P,
    signature: String,
    fields: T,
) -> Vector<T> {
    let other_intent_signature = ctx
        .sign_intent_at(other_intent, TIMESTAMP_MS, payload.clone())
        .expect("sign")
        .signature;
    Vector {
        intent: intent.0,
        timestamp_ms: TIMESTAMP_MS,
        fields,
        payload_bcs_hex: hex::encode(bcs::to_bytes(&payload).expect("BCS")),
        intent_message_hex: hex::encode(
            intent_message(intent, TIMESTAMP_MS, &payload).expect("BCS"),
        ),
        public_key: ctx.public_key_hex(),
        signature,
        other_intent_signature,
    }
}

fn audio_vector() -> (Vector<AudioJson>, NautilusContext) {
    let ctx = context();
    let body = signed(&ctx);
    let audio = vector(
        &ctx,
        AUDIO_INTENT,
        OTHER_INTENT,
        fixed_audio(),
        body.audio.signature,
        body.audio.data,
    );
    (audio, ctx)
}

fn facts_bcs() -> Vec<u8> {
    let mut expected = vec![2, 24];
    expected.extend(48_000u32.to_le_bytes());
    expected.extend(480_000u64.to_le_bytes());
    expected.push(32);
    expected.extend([0x11; 32]);
    expected
}

#[test]
fn audio_payload_bcs_matches_move_layout() {
    let payload = bcs::to_bytes(&fixed_audio()).expect("BCS");
    assert_eq!(payload, facts_bcs());
    assert_eq!(payload.len(), 1 + 1 + 4 + 8 + 33);
}

fn assert_signs<T>(vector: &Vector<T>, ctx: &NautilusContext, intent: Intent) {
    let message = hex::decode(&vector.intent_message_hex).expect("hex");
    assert_eq!(message[0], intent.0);
    assert_eq!(&message[1..9], &TIMESTAMP_MS.to_le_bytes());
    assert_eq!(
        hex::encode(&message[9..]),
        vector.payload_bcs_hex,
        "intent message is intent || timestamp || payload"
    );
    let key = ed25519_dalek::VerifyingKey::from_bytes(&ctx.public_key()).expect("key");
    let signature = |hex_signature: &str| {
        let bytes: [u8; 64] = hex::decode(hex_signature)
            .expect("hex")
            .try_into()
            .expect("64 bytes");
        ed25519_dalek::Signature::from_bytes(&bytes)
    };
    key.verify_strict(&message, &signature(&vector.signature))
        .expect("signature verifies");
    assert!(
        key.verify_strict(&message, &signature(&vector.other_intent_signature))
            .is_err(),
        "a signature under the other intent does not verify"
    );
}

#[test]
fn vector_signature_verifies_under_its_intent_only() {
    let (audio, ctx) = audio_vector();
    assert_signs(&audio, &ctx, AUDIO_INTENT);
}

fn assert_committed<T: Serialize>(name: &str, vector: &T) {
    let rendered = serde_json::to_string_pretty(vector).expect("JSON") + "\n";
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../vectors")
        .join(name);
    let committed = std::fs::read_to_string(&path).ok();
    if committed.as_deref() != Some(rendered.as_str()) {
        std::fs::write(&path, &rendered).expect("write vector");
        panic!("vectors/{name} was stale and has been regenerated; review and commit it");
    }
}

#[test]
fn committed_vector_is_up_to_date() {
    let (audio, _) = audio_vector();
    assert_committed("audio_intent.json", &audio);
}
