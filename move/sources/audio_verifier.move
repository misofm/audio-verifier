// Copyright (c) Miso Labs, Inc.
// SPDX-License-Identifier: FSL-1.1-ALv2

module audio_verifier::audio_verifier;

use audio::{audio::{Self, Audio, AudioRegistry}, audio_encoding};
use kagi::{enclave::Enclave, enclave_policy};
use std::string::String;
use sui::event::emit;

// === Errors ===

const EWrongAudio: u64 = 0;

// === Constants ===

const AUDIO_INTENT: u8 = 0;

// === Structs ===

public struct AUDIO_VERIFIER() has drop;

public struct AudioVerifier() has drop;

public struct AudioVerificationPayload has copy, drop {
    format: String,
    channels: u8,
    bit_depth: u8,
    sample_rate_hz: u32,
    samples_per_channel: u64,
    pcm_digest: vector<u8>,
    blob_id: u256,
}

// === Events ===

public struct AudioVerifiedEvent has copy, drop {
    enclave_id: ID,
    policy_version: u64,
    verified_at_ms: u64,
    audio_id: ID,
    blob_id: u256,
}

// === Public Functions ===

fun init(otw: AUDIO_VERIFIER, ctx: &mut TxContext) {
    let (policy, cap) = enclave_policy::new(otw, ctx);
    policy.share();
    transfer::public_transfer(cap, ctx.sender());
}

public fun verify_audio(
    registry: &mut AudioRegistry,
    enclave: &Enclave<AUDIO_VERIFIER>,
    format: String,
    channels: u8,
    bit_depth: u8,
    sample_rate_hz: u32,
    samples_per_channel: u64,
    pcm_digest: vector<u8>,
    blob_id: u256,
    timestamp_ms: u64,
    signature: vector<u8>,
): Audio<AudioVerifier> {
    enclave.verify_signature(
        AUDIO_INTENT,
        timestamp_ms,
        AudioVerificationPayload {
            format,
            channels,
            bit_depth,
            sample_rate_hz,
            samples_per_channel,
            pcm_digest,
            blob_id,
        },
        &signature,
    );
    let mut audio = audio::new(
        registry,
        AudioVerifier(),
        channels,
        bit_depth,
        sample_rate_hz,
        samples_per_channel,
        pcm_digest,
    );
    register_verified_encoding(&mut audio, enclave, blob_id, timestamp_ms);
    audio
}

public fun verify_encoding(
    audio: &mut Audio<AudioVerifier>,
    enclave: &Enclave<AUDIO_VERIFIER>,
    format: String,
    channels: u8,
    bit_depth: u8,
    sample_rate_hz: u32,
    samples_per_channel: u64,
    pcm_digest: vector<u8>,
    blob_id: u256,
    timestamp_ms: u64,
    signature: vector<u8>,
) {
    assert!(
        audio.channels() == channels &&
        audio.bit_depth() == bit_depth &&
        audio.sample_rate_hz() == sample_rate_hz &&
        audio.samples_per_channel() == samples_per_channel &&
        audio.pcm_digest() == &pcm_digest,
        EWrongAudio,
    );
    enclave.verify_signature(
        AUDIO_INTENT,
        timestamp_ms,
        AudioVerificationPayload {
            format,
            channels,
            bit_depth,
            sample_rate_hz,
            samples_per_channel,
            pcm_digest,
            blob_id,
        },
        &signature,
    );
    register_verified_encoding(audio, enclave, blob_id, timestamp_ms);
}

// === Private Functions ===

fun register_verified_encoding(
    audio: &mut Audio<AudioVerifier>,
    enclave: &Enclave<AUDIO_VERIFIER>,
    blob_id: u256,
    timestamp_ms: u64,
) {
    audio_encoding::register_encoding(audio, AudioVerifier(), blob_id);
    emit(AudioVerifiedEvent {
        enclave_id: object::id(enclave),
        policy_version: enclave.policy_version(),
        verified_at_ms: timestamp_ms,
        audio_id: object::id(audio),
        blob_id,
    });
}

// === Test Functions ===

#[test_only]
public fun init_for_testing(ctx: &mut TxContext) {
    init(AUDIO_VERIFIER(), ctx);
}

#[test_only]
public fun new_audio_verification_payload_for_testing(
    format: String,
    channels: u8,
    bit_depth: u8,
    sample_rate_hz: u32,
    samples_per_channel: u64,
    pcm_digest: vector<u8>,
    blob_id: u256,
): AudioVerificationPayload {
    AudioVerificationPayload {
        format,
        channels,
        bit_depth,
        sample_rate_hz,
        samples_per_channel,
        pcm_digest,
        blob_id,
    }
}

#[test_only]
public fun audio_verified_event_fields(event: &AudioVerifiedEvent): (ID, u64, u64, ID, u256) {
    (event.enclave_id, event.policy_version, event.verified_at_ms, event.audio_id, event.blob_id)
}
