// Copyright (c) Miso Labs, Inc.
// SPDX-License-Identifier: FSL-1.1-ALv2

module audio_verifier::audio_verifier;

use audio::{audio::{Self, Audio, AudioRegistry}, audio_encoding};
use kagi::{enclave::Enclave, enclave_policy};
use sui::event::emit;

// === Constants ===

const AUDIO_INTENT: u8 = 0;
const AUDIO_ENCODING_INTENT: u8 = 1;

// === Structs ===

public struct AUDIO_VERIFIER() has drop;

public struct AudioVerifier() has drop;

public struct AudioPayload has copy, drop {
    channels: u8,
    bit_depth: u8,
    sample_rate_hz: u32,
    samples_per_channel: u64,
    pcm_digest: vector<u8>,
}

public struct AudioEncodingPayload has copy, drop {
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
}

public struct AudioEncodingVerifiedEvent has copy, drop {
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
    channels: u8,
    bit_depth: u8,
    sample_rate_hz: u32,
    samples_per_channel: u64,
    pcm_digest: vector<u8>,
    timestamp_ms: u64,
    signature: vector<u8>,
): Audio<AudioVerifier> {
    enclave.verify_signature(
        AUDIO_INTENT,
        timestamp_ms,
        AudioPayload { channels, bit_depth, sample_rate_hz, samples_per_channel, pcm_digest },
        &signature,
    );
    let audio = audio::new(
        registry,
        AudioVerifier(),
        channels,
        bit_depth,
        sample_rate_hz,
        samples_per_channel,
        pcm_digest,
    );
    emit(AudioVerifiedEvent {
        enclave_id: object::id(enclave),
        policy_version: enclave.policy_version(),
        verified_at_ms: timestamp_ms,
        audio_id: object::id(&audio),
    });
    audio
}

public fun verify_encoding(
    audio: &mut Audio<AudioVerifier>,
    enclave: &Enclave<AUDIO_VERIFIER>,
    blob_id: u256,
    timestamp_ms: u64,
    signature: vector<u8>,
) {
    enclave.verify_signature(
        AUDIO_ENCODING_INTENT,
        timestamp_ms,
        AudioEncodingPayload {
            channels: audio.channels(),
            bit_depth: audio.bit_depth(),
            sample_rate_hz: audio.sample_rate_hz(),
            samples_per_channel: audio.samples_per_channel(),
            pcm_digest: *audio.pcm_digest(),
            blob_id,
        },
        &signature,
    );
    audio_encoding::register_encoding(audio, AudioVerifier(), blob_id);
    emit(AudioEncodingVerifiedEvent {
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
public fun new_audio_for_testing(
    registry: &mut AudioRegistry,
    channels: u8,
    bit_depth: u8,
    sample_rate_hz: u32,
    samples_per_channel: u64,
    pcm_digest: vector<u8>,
): Audio<AudioVerifier> {
    audio::new(
        registry,
        AudioVerifier(),
        channels,
        bit_depth,
        sample_rate_hz,
        samples_per_channel,
        pcm_digest,
    )
}

#[test_only]
public fun new_audio_payload_for_testing(
    channels: u8,
    bit_depth: u8,
    sample_rate_hz: u32,
    samples_per_channel: u64,
    pcm_digest: vector<u8>,
): AudioPayload {
    AudioPayload { channels, bit_depth, sample_rate_hz, samples_per_channel, pcm_digest }
}

#[test_only]
public fun new_audio_encoding_payload_for_testing(
    channels: u8,
    bit_depth: u8,
    sample_rate_hz: u32,
    samples_per_channel: u64,
    pcm_digest: vector<u8>,
    blob_id: u256,
): AudioEncodingPayload {
    AudioEncodingPayload {
        channels,
        bit_depth,
        sample_rate_hz,
        samples_per_channel,
        pcm_digest,
        blob_id,
    }
}

#[test_only]
public fun audio_verified_event_fields(event: &AudioVerifiedEvent): (ID, u64, u64, ID) {
    (event.enclave_id, event.policy_version, event.verified_at_ms, event.audio_id)
}

#[test_only]
public fun audio_encoding_verified_event_fields(
    event: &AudioEncodingVerifiedEvent,
): (ID, u64, u64, ID, u256) {
    (event.enclave_id, event.policy_version, event.verified_at_ms, event.audio_id, event.blob_id)
}
