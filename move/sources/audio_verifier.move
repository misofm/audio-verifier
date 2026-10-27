// Copyright (c) Miso Labs, Inc.
// SPDX-License-Identifier: FSL-1.1-ALv2

module audio_verifier::audio_verifier;

// === Imports ===

use std::string::String;
use sui::event::emit;
use audio::audio::{Self, Audio};
use kagi::enclave::Enclave;
use kagi::enclave_policy;
use ori::{confidentiality, data};

// === Constants ===

const AUDIO_INTENT: u8 = 0;

// === Structs ===

public struct AUDIO_VERIFIER() has drop;

public struct AudioVerifier() has drop;

public struct AudioPayload has copy, drop {
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
    pcm_digest: vector<u8>,
    blob_id: u256,
}

// === Public Functions ===

fun init(otw: AUDIO_VERIFIER, ctx: &mut TxContext) {
    let (policy, cap) = enclave_policy::new(otw, ctx);
    policy.share();
    transfer::public_transfer(cap, ctx.sender());
}

public fun verify(
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
        AudioPayload {
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
    emit(AudioVerifiedEvent {
        enclave_id: object::id(enclave),
        policy_version: enclave.policy_version(),
        verified_at_ms: timestamp_ms,
        pcm_digest,
        blob_id,
    });
    audio::new(
        AudioVerifier(),
        format,
        channels,
        bit_depth,
        sample_rate_hz,
        samples_per_channel,
        pcm_digest,
        data::new_blob(blob_id, confidentiality::new_unencrypted()),
    )
}

// === Test Functions ===

#[test_only]
public fun init_for_testing(ctx: &mut TxContext) {
    init(AUDIO_VERIFIER(), ctx);
}

#[test_only]
public fun new_audio_payload_for_testing(
    format: String,
    channels: u8,
    bit_depth: u8,
    sample_rate_hz: u32,
    samples_per_channel: u64,
    pcm_digest: vector<u8>,
    blob_id: u256,
): AudioPayload {
    AudioPayload {
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
public fun audio_verified_event_fields(
    event: &AudioVerifiedEvent,
): (ID, u64, u64, vector<u8>, u256) {
    (event.enclave_id, event.policy_version, event.verified_at_ms, event.pcm_digest, event.blob_id)
}
