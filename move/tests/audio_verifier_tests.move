// Copyright (c) Miso Labs, Inc.
// SPDX-License-Identifier: FSL-1.1-ALv2

#[test_only]
module audio_verifier::audio_verifier_tests;

use audio::{
    audio::{Self, Audio, AudioRegistry, AudioCreatedEvent},
    audio_encoding::{Self, AudioEncodingRegisteredEvent}
};
use audio_verifier::audio_verifier::{
    Self,
    AUDIO_VERIFIER,
    AudioVerifier,
    AudioVerifiedEvent,
    AudioEncodingVerifiedEvent,
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
const AUDIO_PAYLOAD: vector<u8> =
    x"021880bb00000053070000000000201111111111111111111111111111111111111111111111111111111111111111";
const AUDIO_MESSAGE: vector<u8> =
    x"0000dc207497010000021880bb00000053070000000000201111111111111111111111111111111111111111111111111111111111111111";
const AUDIO_SIGNATURE: vector<u8> =
    x"eeb10727367e2bfe4fe5ebe15275562f01531e4749a3a9822b40c427d6b95b7dfd324a26a8e9c4b80a379e3c66457935514a1bb912cf4bf85f255afa4b0d0709";
const AUDIO_OTHER_INTENT_SIGNATURE: vector<u8> =
    x"4f08c5a99b14a40ae5921f60bb945f8e40373608e97bc0557819ccf87ed6a4cf8a14f4249cc4652d14074ce205a919c761fde7ed5d6140514699ab5295622f04";
const ENCODING_PAYLOAD: vector<u8> =
    x"021880bb00000053070000000000201111111111111111111111111111111111111111111111111111111111111111000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f";
const ENCODING_MESSAGE: vector<u8> =
    x"0100dc207497010000021880bb00000053070000000000201111111111111111111111111111111111111111111111111111111111111111000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f";
const ENCODING_SIGNATURE: vector<u8> =
    x"25bf0d9ec6702100f2081d95bd968f5135eb3a292c1de0e1d09054a1d668018ed40eeb60907b14c6ae70beaffcf0e577bd36531c55c76c21063260c65cb2cd0c";
const ENCODING_OTHER_INTENT_SIGNATURE: vector<u8> =
    x"57fab9d4f969a7b670d3b1f7f7c17ae7ee8aa780aa610b142e94e1828a45e4b2b527276ac5e5f0dcb9ac66ea83434fbea34827d71fc79ec748d9516705813903";

// === Helpers ===

fun enclave(scenario: &mut Scenario): Enclave<AUDIO_VERIFIER> {
    enclave::new_for_testing<AUDIO_VERIFIER>(PK, POLICY_VERSION, scenario.ctx())
}

fun verify_audio_with(
    registry: &mut AudioRegistry,
    enclave: &Enclave<AUDIO_VERIFIER>,
    samples_per_channel: u64,
    timestamp_ms: u64,
    signature: vector<u8>,
): Audio<AudioVerifier> {
    audio_verifier::verify_audio(
        registry,
        enclave,
        CHANNELS,
        BIT_DEPTH,
        SAMPLE_RATE_HZ,
        samples_per_channel,
        PCM_DIGEST,
        timestamp_ms,
        signature,
    )
}

fun verified(
    scenario: &mut Scenario,
): (AudioRegistry, Enclave<AUDIO_VERIFIER>, Audio<AudioVerifier>) {
    let mut registry = audio::new_registry_for_testing(scenario.ctx());
    let enclave = enclave(scenario);
    let audio = verify_audio_with(
        &mut registry,
        &enclave,
        SAMPLES_PER_CHANNEL,
        SIGNED_AT_MS,
        AUDIO_SIGNATURE,
    );
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
fun audio_payload_bcs_matches_the_signed_vector() {
    let payload = audio_verifier::new_audio_payload_for_testing(
        CHANNELS,
        BIT_DEPTH,
        SAMPLE_RATE_HZ,
        SAMPLES_PER_CHANNEL,
        PCM_DIGEST,
    );
    assert_eq!(bcs::to_bytes(&payload), AUDIO_PAYLOAD);
    let message = enclave::create_intent_message(0, SIGNED_AT_MS, payload);
    assert_eq!(bcs::to_bytes(&message), AUDIO_MESSAGE);
}

#[test]
fun audio_encoding_payload_bcs_matches_the_signed_vector() {
    let payload = audio_verifier::new_audio_encoding_payload_for_testing(
        CHANNELS,
        BIT_DEPTH,
        SAMPLE_RATE_HZ,
        SAMPLES_PER_CHANNEL,
        PCM_DIGEST,
        BLOB_ID,
    );
    assert_eq!(bcs::to_bytes(&payload), ENCODING_PAYLOAD);
    let message = enclave::create_intent_message(1, SIGNED_AT_MS, payload);
    assert_eq!(bcs::to_bytes(&message), ENCODING_MESSAGE);
}

// === Verify Audio ===

#[test]
fun verify_audio_returns_the_signed_facts_without_an_encoding() {
    let mut scenario = test_scenario::begin(PUBLISHER);
    let (registry, enclave, audio) = verified(&mut scenario);
    assert_eq!(audio.channels(), CHANNELS);
    assert_eq!(audio.bit_depth(), BIT_DEPTH);
    assert_eq!(audio.sample_rate_hz(), SAMPLE_RATE_HZ);
    assert_eq!(audio.samples_per_channel(), SAMPLES_PER_CHANNEL);
    assert_eq!(*audio.pcm_digest(), PCM_DIGEST);
    assert_eq!(audio.duration_ms(), 10_000);
    assert!(!audio_encoding::has_encoding<_, AudioVerifier>(&audio, BLOB_ID));
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
fun verify_audio_emits_the_verification_and_the_audio() {
    let mut scenario = test_scenario::begin(PUBLISHER);
    let (registry, enclave, audio) = verified(&mut scenario);
    let events = event::events_by_type<AudioVerifiedEvent>();
    assert_eq!(events.length(), 1);
    let (
        enclave_id,
        policy_version,
        verified_at_ms,
        audio_id,
    ) = audio_verifier::audio_verified_event_fields(&events[0]);
    assert_eq!(enclave_id, object::id(&enclave));
    assert_eq!(policy_version, POLICY_VERSION);
    assert_eq!(verified_at_ms, SIGNED_AT_MS);
    assert_eq!(audio_id, object::id(&audio));
    assert_eq!(event::events_by_type<AudioCreatedEvent<AudioVerifier>>().length(), 1);
    assert_eq!(event::num_events(), 2);
    finish(registry, enclave, audio);
    scenario.end();
}

#[test, expected_failure(abort_code = audio::EAudioExists)]
fun verify_audio_rejects_registered_audio() {
    let mut scenario = test_scenario::begin(PUBLISHER);
    let (mut registry, enclave, _audio) = verified(&mut scenario);
    let _again = verify_audio_with(
        &mut registry,
        &enclave,
        SAMPLES_PER_CHANNEL,
        SIGNED_AT_MS,
        AUDIO_SIGNATURE,
    );
    abort
}

#[test, expected_failure(abort_code = enclave::EInvalidSignature)]
fun verify_audio_rejects_facts_the_enclave_did_not_sign() {
    let mut scenario = test_scenario::begin(PUBLISHER);
    let mut registry = audio::new_registry_for_testing(scenario.ctx());
    let enclave = enclave(&mut scenario);
    let _audio = verify_audio_with(
        &mut registry,
        &enclave,
        SAMPLES_PER_CHANNEL + 1,
        SIGNED_AT_MS,
        AUDIO_SIGNATURE,
    );
    abort
}

#[test, expected_failure(abort_code = enclave::EInvalidSignature)]
fun verify_audio_rejects_another_timestamp() {
    let mut scenario = test_scenario::begin(PUBLISHER);
    let mut registry = audio::new_registry_for_testing(scenario.ctx());
    let enclave = enclave(&mut scenario);
    let _audio = verify_audio_with(
        &mut registry,
        &enclave,
        SAMPLES_PER_CHANNEL,
        SIGNED_AT_MS + 1,
        AUDIO_SIGNATURE,
    );
    abort
}

#[test, expected_failure(abort_code = enclave::EInvalidSignature)]
fun verify_audio_rejects_a_bad_signature() {
    let mut scenario = test_scenario::begin(PUBLISHER);
    let mut registry = audio::new_registry_for_testing(scenario.ctx());
    let enclave = enclave(&mut scenario);
    let mut signature = AUDIO_SIGNATURE;
    let byte = &mut signature[0];
    *byte = *byte ^ 1;
    let _audio = verify_audio_with(
        &mut registry,
        &enclave,
        SAMPLES_PER_CHANNEL,
        SIGNED_AT_MS,
        signature,
    );
    abort
}

#[test, expected_failure(abort_code = enclave::EInvalidSignature)]
fun verify_audio_rejects_the_facts_signed_under_the_encoding_intent() {
    let mut scenario = test_scenario::begin(PUBLISHER);
    let mut registry = audio::new_registry_for_testing(scenario.ctx());
    let enclave = enclave(&mut scenario);
    let _audio = verify_audio_with(
        &mut registry,
        &enclave,
        SAMPLES_PER_CHANNEL,
        SIGNED_AT_MS,
        AUDIO_OTHER_INTENT_SIGNATURE,
    );
    abort
}

#[test, expected_failure(abort_code = enclave::EInvalidSignature)]
fun verify_audio_rejects_an_encoding_signature() {
    let mut scenario = test_scenario::begin(PUBLISHER);
    let mut registry = audio::new_registry_for_testing(scenario.ctx());
    let enclave = enclave(&mut scenario);
    let _audio = verify_audio_with(
        &mut registry,
        &enclave,
        SAMPLES_PER_CHANNEL,
        SIGNED_AT_MS,
        ENCODING_SIGNATURE,
    );
    abort
}

#[test, expected_failure(abort_code = enclave::EInvalidSignature)]
fun verify_audio_rejects_another_enclave_key() {
    let mut scenario = test_scenario::begin(PUBLISHER);
    let mut registry = audio::new_registry_for_testing(scenario.ctx());
    let enclave = enclave::new_for_testing<AUDIO_VERIFIER>(
        x"6e7a1cdd29b0b78fd13af4c5598feff4ef2a97166e3ca6f2e4fbfccd80505bf1",
        POLICY_VERSION,
        scenario.ctx(),
    );
    let _audio = verify_audio_with(
        &mut registry,
        &enclave,
        SAMPLES_PER_CHANNEL,
        SIGNED_AT_MS,
        AUDIO_SIGNATURE,
    );
    abort
}

// === Verify Encoding ===

#[test]
fun verify_encoding_registers_the_blob_and_emits_the_verification() {
    let mut scenario = test_scenario::begin(PUBLISHER);
    let (registry, enclave, mut audio) = verified(&mut scenario);
    audio_verifier::verify_encoding(
        &mut audio,
        &enclave,
        BLOB_ID,
        SIGNED_AT_MS,
        ENCODING_SIGNATURE,
    );
    assert!(audio_encoding::has_encoding<_, AudioVerifier>(&audio, BLOB_ID));
    let events = event::events_by_type<AudioEncodingVerifiedEvent>();
    assert_eq!(events.length(), 1);
    let (
        enclave_id,
        policy_version,
        verified_at_ms,
        audio_id,
        blob_id,
    ) = audio_verifier::audio_encoding_verified_event_fields(&events[0]);
    assert_eq!(enclave_id, object::id(&enclave));
    assert_eq!(policy_version, POLICY_VERSION);
    assert_eq!(verified_at_ms, SIGNED_AT_MS);
    assert_eq!(audio_id, object::id(&audio));
    assert_eq!(blob_id, BLOB_ID);
    assert_eq!(
        event::events_by_type<
            AudioEncodingRegisteredEvent<AudioVerifier, AudioVerifier>,
        >().length(),
        1,
    );
    finish(registry, enclave, audio);
    scenario.end();
}

#[test]
fun verifying_an_encoding_again_registers_it_once() {
    let mut scenario = test_scenario::begin(PUBLISHER);
    let (registry, enclave, mut audio) = verified(&mut scenario);
    audio_verifier::verify_encoding(
        &mut audio,
        &enclave,
        BLOB_ID,
        SIGNED_AT_MS,
        ENCODING_SIGNATURE,
    );
    audio_verifier::verify_encoding(
        &mut audio,
        &enclave,
        BLOB_ID,
        SIGNED_AT_MS,
        ENCODING_SIGNATURE,
    );
    assert_eq!(event::events_by_type<AudioEncodingVerifiedEvent>().length(), 2);
    assert_eq!(
        event::events_by_type<
            AudioEncodingRegisteredEvent<AudioVerifier, AudioVerifier>,
        >().length(),
        1,
    );
    finish(registry, enclave, audio);
    scenario.end();
}

#[test, expected_failure(abort_code = enclave::EInvalidSignature)]
fun verify_encoding_rejects_audio_with_other_facts() {
    let mut scenario = test_scenario::begin(PUBLISHER);
    let mut registry = audio::new_registry_for_testing(scenario.ctx());
    let enclave = enclave(&mut scenario);
    let mut other = audio_verifier::new_audio_for_testing(
        &mut registry,
        CHANNELS,
        BIT_DEPTH,
        44_100,
        SAMPLES_PER_CHANNEL,
        PCM_DIGEST,
    );
    audio_verifier::verify_encoding(
        &mut other,
        &enclave,
        BLOB_ID,
        SIGNED_AT_MS,
        ENCODING_SIGNATURE,
    );
    abort
}

#[test, expected_failure(abort_code = enclave::EInvalidSignature)]
fun verify_encoding_rejects_another_blob() {
    let mut scenario = test_scenario::begin(PUBLISHER);
    let (_registry, enclave, mut audio) = verified(&mut scenario);
    audio_verifier::verify_encoding(
        &mut audio,
        &enclave,
        BLOB_ID + 1,
        SIGNED_AT_MS,
        ENCODING_SIGNATURE,
    );
    abort
}

#[test, expected_failure(abort_code = enclave::EInvalidSignature)]
fun verify_encoding_rejects_another_timestamp() {
    let mut scenario = test_scenario::begin(PUBLISHER);
    let (_registry, enclave, mut audio) = verified(&mut scenario);
    audio_verifier::verify_encoding(
        &mut audio,
        &enclave,
        BLOB_ID,
        SIGNED_AT_MS + 1,
        ENCODING_SIGNATURE,
    );
    abort
}

#[test, expected_failure(abort_code = enclave::EInvalidSignature)]
fun verify_encoding_rejects_the_encoding_signed_under_the_audio_intent() {
    let mut scenario = test_scenario::begin(PUBLISHER);
    let (_registry, enclave, mut audio) = verified(&mut scenario);
    audio_verifier::verify_encoding(
        &mut audio,
        &enclave,
        BLOB_ID,
        SIGNED_AT_MS,
        ENCODING_OTHER_INTENT_SIGNATURE,
    );
    abort
}

#[test, expected_failure(abort_code = enclave::EInvalidSignature)]
fun verify_encoding_rejects_an_audio_signature() {
    let mut scenario = test_scenario::begin(PUBLISHER);
    let (_registry, enclave, mut audio) = verified(&mut scenario);
    audio_verifier::verify_encoding(&mut audio, &enclave, BLOB_ID, SIGNED_AT_MS, AUDIO_SIGNATURE);
    abort
}
