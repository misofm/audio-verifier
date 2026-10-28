// Copyright (c) Miso Labs, Inc.
// SPDX-License-Identifier: FSL-1.1-ALv2

#[test_only]
module audio_verifier::audio_verifier_tests;

use audio::audio::{Self, Audio, AudioRegistry, AudioCreatedEvent, AudioEncodingRegisteredEvent};
use audio_verifier::audio_verifier::{
    Self,
    AUDIO_VERIFIER,
    AudioVerifier,
    AudioVerificationPayload,
    AudioVerifiedEvent,
};
use kagi::{enclave::{Self, Enclave}, enclave_policy::{EnclavePolicy, EnclavePolicyCap}};
use std::{bcs, unit_test::{assert_eq, destroy}};
use sui::{event, test_scenario::{Self, Scenario}};

// === Constants ===

const PUBLISHER: address = @0xA;

const PK: vector<u8> = x"ea4a6c63e29c520abef5507b132ec5f9954776aebebe7b92421eea691446d22c";
const SIGNED_AT_MS: u64 = 1_750_000_000_000;
const POLICY_VERSION: u64 = 1;
const CHANNELS: u8 = 2;
const BIT_DEPTH: u8 = 24;
const SAMPLE_RATE_HZ: u32 = 48_000;
const SAMPLES_PER_CHANNEL: u64 = 480_000;
const PCM_DIGEST: vector<u8> = x"1111111111111111111111111111111111111111111111111111111111111111";
const BLOB_ID: u256 = 14074904626401341155369551180448584754667373453244490859944217516317499064576;
const PAYLOAD: vector<u8> =
    x"04666c6163021880bb00000053070000000000201111111111111111111111111111111111111111111111111111111111111111000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f";
const MESSAGE: vector<u8> =
    x"0000dc20749701000004666c6163021880bb00000053070000000000201111111111111111111111111111111111111111111111111111111111111111000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f";
const SIGNATURE: vector<u8> =
    x"a4e4bcfbed23a6ff96595d3d5392f08338941cc20dc1337630761292bb118d2e17bbb3299b3fcb0054ba072a7fd3ed69d0c6124507e8e1d34febad9e7edc2d0e";
const INTENT_ONE_SIGNATURE: vector<u8> =
    x"85a801b3cd183407ed319ed24d59d766b52c10abfde14a05ce3f3c60e8425feb326d869054e180a56874aa53826c5573b504463d723112e11679fbf2aae5ad0c";

// === Helpers ===

fun enclave(scenario: &mut Scenario): Enclave<AUDIO_VERIFIER> {
    enclave::new_for_testing<AUDIO_VERIFIER>(PK, POLICY_VERSION, scenario.ctx())
}

fun payload(): AudioVerificationPayload {
    audio_verifier::new_audio_verification_payload_for_testing(
        b"flac".to_string(),
        CHANNELS,
        BIT_DEPTH,
        SAMPLE_RATE_HZ,
        SAMPLES_PER_CHANNEL,
        PCM_DIGEST,
        BLOB_ID,
    )
}

fun verify_with(
    registry: &mut AudioRegistry,
    enclave: &Enclave<AUDIO_VERIFIER>,
    samples_per_channel: u64,
    timestamp_ms: u64,
    signature: vector<u8>,
): Audio<AudioVerifier> {
    audio_verifier::verify_audio(
        registry,
        enclave,
        b"flac".to_string(),
        CHANNELS,
        BIT_DEPTH,
        SAMPLE_RATE_HZ,
        samples_per_channel,
        PCM_DIGEST,
        BLOB_ID,
        timestamp_ms,
        signature,
    )
}

fun verify_encoding_with(
    audio: &mut Audio<AudioVerifier>,
    enclave: &Enclave<AUDIO_VERIFIER>,
    channels: u8,
    bit_depth: u8,
    sample_rate_hz: u32,
    samples_per_channel: u64,
    pcm_digest: vector<u8>,
) {
    audio_verifier::verify_encoding(
        audio,
        enclave,
        b"flac".to_string(),
        channels,
        bit_depth,
        sample_rate_hz,
        samples_per_channel,
        pcm_digest,
        BLOB_ID,
        SIGNED_AT_MS,
        SIGNATURE,
    )
}

fun verified(
    scenario: &mut Scenario,
): (AudioRegistry, Enclave<AUDIO_VERIFIER>, Audio<AudioVerifier>) {
    let mut registry = audio::new_registry_for_testing(scenario.ctx());
    let enclave = enclave(scenario);
    let audio = verify_with(&mut registry, &enclave, SAMPLES_PER_CHANNEL, SIGNED_AT_MS, SIGNATURE);
    (registry, enclave, audio)
}

fun finish(registry: AudioRegistry, enclave: Enclave<AUDIO_VERIFIER>, audio: Audio<AudioVerifier>) {
    destroy(registry);
    enclave.destroy_for_testing();
    destroy(audio);
}

// === Init ===

#[test]
fun init_shares_an_unset_policy_and_sends_its_cap() {
    let mut scenario = test_scenario::begin(PUBLISHER);
    audio_verifier::init_for_testing(scenario.ctx());
    scenario.next_tx(PUBLISHER);
    let policy = scenario.take_shared<EnclavePolicy<AUDIO_VERIFIER>>();
    assert!(policy.pcr0().is_empty());
    assert_eq!(policy.version(), 0);
    test_scenario::return_shared(policy);
    let cap = scenario.take_from_sender<EnclavePolicyCap<AUDIO_VERIFIER>>();
    destroy(cap);
    scenario.end();
}

// === Layout ===

#[test]
fun payload_bcs_matches_the_signed_vector() {
    assert_eq!(bcs::to_bytes(&payload()), PAYLOAD);
    let message = enclave::create_intent_message(0, SIGNED_AT_MS, payload());
    assert_eq!(bcs::to_bytes(&message), MESSAGE);
}

// === Verify ===

#[test]
fun verify_audio_returns_the_signed_facts_and_encoding() {
    let mut scenario = test_scenario::begin(PUBLISHER);
    let (registry, enclave, audio) = verified(&mut scenario);
    assert_eq!(audio.channels(), CHANNELS);
    assert_eq!(audio.bit_depth(), BIT_DEPTH);
    assert_eq!(audio.sample_rate_hz(), SAMPLE_RATE_HZ);
    assert_eq!(audio.samples_per_channel(), SAMPLES_PER_CHANNEL);
    assert_eq!(*audio.pcm_digest(), PCM_DIGEST);
    assert_eq!(audio.duration_ms(), 10_000);
    assert!(audio.has_encoding<_, AudioVerifier>(BLOB_ID));
    assert!(!audio.encoding<_, AudioVerifier>(BLOB_ID).blob_confidentiality().is_encrypted());
    assert_eq!(
        object::id_address(&audio),
        audio::derive_address<AudioVerifier>(
            object::id(&registry),
            CHANNELS,
            BIT_DEPTH,
            SAMPLE_RATE_HZ,
            SAMPLES_PER_CHANNEL,
            PCM_DIGEST,
        ),
    );
    finish(registry, enclave, audio);
    scenario.end();
}

#[test]
fun verify_audio_emits_the_verification_the_audio_and_the_encoding() {
    let mut scenario = test_scenario::begin(PUBLISHER);
    let (registry, enclave, audio) = verified(&mut scenario);
    let events = event::events_by_type<AudioVerifiedEvent>();
    assert_eq!(events.length(), 1);
    let (
        enclave_id,
        policy_version,
        verified_at_ms,
        audio_id,
        blob_id,
    ) = audio_verifier::audio_verified_event_fields(&events[0]);
    assert_eq!(enclave_id, object::id(&enclave));
    assert_eq!(policy_version, POLICY_VERSION);
    assert_eq!(verified_at_ms, SIGNED_AT_MS);
    assert_eq!(audio_id, object::id(&audio));
    assert_eq!(blob_id, BLOB_ID);
    assert_eq!(event::events_by_type<AudioCreatedEvent<AudioVerifier>>().length(), 1);
    assert_eq!(
        event::events_by_type<
            AudioEncodingRegisteredEvent<AudioVerifier, AudioVerifier>,
        >().length(),
        1,
    );
    assert_eq!(event::num_events(), 3);
    finish(registry, enclave, audio);
    scenario.end();
}

#[test, expected_failure(abort_code = audio::EAudioExists)]
fun verify_audio_rejects_registered_audio() {
    let mut scenario = test_scenario::begin(PUBLISHER);
    let (mut registry, enclave, _audio) = verified(&mut scenario);
    let _again = verify_with(&mut registry, &enclave, SAMPLES_PER_CHANNEL, SIGNED_AT_MS, SIGNATURE);
    abort
}

#[test]
fun verify_encoding_on_existing_audio_records_the_verification() {
    let mut scenario = test_scenario::begin(PUBLISHER);
    let (registry, enclave, mut audio) = verified(&mut scenario);
    verify_encoding_with(
        &mut audio,
        &enclave,
        CHANNELS,
        BIT_DEPTH,
        SAMPLE_RATE_HZ,
        SAMPLES_PER_CHANNEL,
        PCM_DIGEST,
    );
    assert!(audio.has_encoding<_, AudioVerifier>(BLOB_ID));
    assert_eq!(event::events_by_type<AudioVerifiedEvent>().length(), 2);
    assert_eq!(
        event::events_by_type<
            AudioEncodingRegisteredEvent<AudioVerifier, AudioVerifier>,
        >().length(),
        1,
    );
    finish(registry, enclave, audio);
    scenario.end();
}

#[test, expected_failure(abort_code = audio_verifier::EWrongAudio)]
fun verify_encoding_rejects_another_sample_rate() {
    let mut scenario = test_scenario::begin(PUBLISHER);
    let (_registry, enclave, mut audio) = verified(&mut scenario);
    verify_encoding_with(
        &mut audio,
        &enclave,
        CHANNELS,
        BIT_DEPTH,
        SAMPLE_RATE_HZ + 1,
        SAMPLES_PER_CHANNEL,
        PCM_DIGEST,
    );
    abort
}

#[test, expected_failure(abort_code = audio_verifier::EWrongAudio)]
fun verify_encoding_rejects_another_channels() {
    let mut scenario = test_scenario::begin(PUBLISHER);
    let (_registry, enclave, mut audio) = verified(&mut scenario);
    verify_encoding_with(
        &mut audio,
        &enclave,
        CHANNELS + 1,
        BIT_DEPTH,
        SAMPLE_RATE_HZ,
        SAMPLES_PER_CHANNEL,
        PCM_DIGEST,
    );
    abort
}

#[test, expected_failure(abort_code = audio_verifier::EWrongAudio)]
fun verify_encoding_rejects_another_bit_depth() {
    let mut scenario = test_scenario::begin(PUBLISHER);
    let (_registry, enclave, mut audio) = verified(&mut scenario);
    verify_encoding_with(
        &mut audio,
        &enclave,
        CHANNELS,
        BIT_DEPTH + 1,
        SAMPLE_RATE_HZ,
        SAMPLES_PER_CHANNEL,
        PCM_DIGEST,
    );
    abort
}

#[test, expected_failure(abort_code = audio_verifier::EWrongAudio)]
fun verify_encoding_rejects_another_samples_per_channel() {
    let mut scenario = test_scenario::begin(PUBLISHER);
    let (_registry, enclave, mut audio) = verified(&mut scenario);
    verify_encoding_with(
        &mut audio,
        &enclave,
        CHANNELS,
        BIT_DEPTH,
        SAMPLE_RATE_HZ,
        SAMPLES_PER_CHANNEL + 1,
        PCM_DIGEST,
    );
    abort
}

#[test, expected_failure(abort_code = audio_verifier::EWrongAudio)]
fun verify_encoding_rejects_another_pcm_digest() {
    let mut scenario = test_scenario::begin(PUBLISHER);
    let (_registry, enclave, mut audio) = verified(&mut scenario);
    verify_encoding_with(
        &mut audio,
        &enclave,
        CHANNELS,
        BIT_DEPTH,
        SAMPLE_RATE_HZ,
        SAMPLES_PER_CHANNEL,
        x"2222222222222222222222222222222222222222222222222222222222222222",
    );
    abort
}

#[test, expected_failure(abort_code = enclave::EInvalidSignature)]
fun verify_rejects_facts_the_enclave_did_not_sign() {
    let mut scenario = test_scenario::begin(PUBLISHER);
    let mut registry = audio::new_registry_for_testing(scenario.ctx());
    let enclave = enclave(&mut scenario);
    let _audio = verify_with(
        &mut registry,
        &enclave,
        SAMPLES_PER_CHANNEL + 1,
        SIGNED_AT_MS,
        SIGNATURE,
    );
    abort
}

#[test, expected_failure(abort_code = enclave::EInvalidSignature)]
fun verify_rejects_another_timestamp() {
    let mut scenario = test_scenario::begin(PUBLISHER);
    let mut registry = audio::new_registry_for_testing(scenario.ctx());
    let enclave = enclave(&mut scenario);
    let _audio = verify_with(
        &mut registry,
        &enclave,
        SAMPLES_PER_CHANNEL,
        SIGNED_AT_MS + 1,
        SIGNATURE,
    );
    abort
}

#[test, expected_failure(abort_code = enclave::EInvalidSignature)]
fun verify_rejects_a_bad_signature() {
    let mut scenario = test_scenario::begin(PUBLISHER);
    let mut registry = audio::new_registry_for_testing(scenario.ctx());
    let enclave = enclave(&mut scenario);
    let mut signature = SIGNATURE;
    let byte = &mut signature[0];
    *byte = *byte ^ 1;
    let _audio = verify_with(&mut registry, &enclave, SAMPLES_PER_CHANNEL, SIGNED_AT_MS, signature);
    abort
}

#[test]
fun the_intent_one_signature_is_valid_for_intent_one() {
    let mut scenario = test_scenario::begin(PUBLISHER);
    let enclave = enclave(&mut scenario);
    let signature = INTENT_ONE_SIGNATURE;
    enclave.verify_signature(1, SIGNED_AT_MS, payload(), &signature);
    enclave.destroy_for_testing();
    scenario.end();
}

#[test, expected_failure(abort_code = enclave::EInvalidSignature)]
fun verify_rejects_a_signature_for_another_intent() {
    let mut scenario = test_scenario::begin(PUBLISHER);
    let mut registry = audio::new_registry_for_testing(scenario.ctx());
    let enclave = enclave(&mut scenario);
    let _audio = verify_with(
        &mut registry,
        &enclave,
        SAMPLES_PER_CHANNEL,
        SIGNED_AT_MS,
        INTENT_ONE_SIGNATURE,
    );
    abort
}

#[test, expected_failure(abort_code = enclave::EInvalidSignature)]
fun verify_rejects_another_enclave_key() {
    let mut scenario = test_scenario::begin(PUBLISHER);
    let mut registry = audio::new_registry_for_testing(scenario.ctx());
    let enclave = enclave::new_for_testing<AUDIO_VERIFIER>(
        x"6e7a1cdd29b0b78fd13af4c5598feff4ef2a97166e3ca6f2e4fbfccd80505bf1",
        POLICY_VERSION,
        scenario.ctx(),
    );
    let _audio = verify_with(&mut registry, &enclave, SAMPLES_PER_CHANNEL, SIGNED_AT_MS, SIGNATURE);
    abort
}

#[test, expected_failure(abort_code = enclave::EInvalidSignature)]
fun verify_encoding_rejects_a_bad_signature() {
    let mut scenario = test_scenario::begin(PUBLISHER);
    let (_registry, enclave, mut audio) = verified(&mut scenario);
    let mut signature = SIGNATURE;
    let byte = &mut signature[0];
    *byte = *byte ^ 1;
    audio_verifier::verify_encoding(
        &mut audio,
        &enclave,
        b"flac".to_string(),
        CHANNELS,
        BIT_DEPTH,
        SAMPLE_RATE_HZ,
        SAMPLES_PER_CHANNEL,
        PCM_DIGEST,
        BLOB_ID,
        SIGNED_AT_MS,
        signature,
    );
    abort
}
