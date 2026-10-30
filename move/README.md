# audio_verifier (Move package)

`verify_audio` creates an [`audio::Audio<AudioVerifier>`](https://github.com/misofm/framework/tree/main/audio), the canonical record of lossless audio measured inside a Nautilus (AWS Nitro) enclave and checked on Sui, and `verify_encoding` registers a file as an encoding of it. The enclave signs the two claims separately. `AudioVerifier` is this package's witness, and only a signature from a registered `Enclave<AUDIO_VERIFIER>` can produce either. Consumers that trust this enclave pin `Audio<audio_verifier::audio_verifier::AudioVerifier>`.

There is no docs.miso.fm page yet, so the facts are below.

```toml
audio_verifier = { git = "https://github.com/misofm/audio-verifier.git", subdir = "move", rev = "<commit>" }
```

## API

| Function | Who may call | What it does |
|---|---|---|
| `init` | publish | Creates the one `kagi::EnclavePolicy<AUDIO_VERIFIER>`, with its PCRs unset, shares it and sends its `EnclavePolicyCap` to the publisher. |
| `verify_audio(registry, enclave, channels, bit_depth, sample_rate_hz, samples_per_channel, pcm_digest, timestamp_ms, signature)` | anyone | Checks the enclave's audio signature, creates the `Audio<AudioVerifier>` from the registry and returns it unshared: register its encoding and attach records to it, then call `audio::share`. Aborts `kagi::enclave::EInvalidSignature`, `derived_object::EObjectAlreadyExists` if the audio is already registered, or another `audio::audio` error if the facts are malformed. |
| `verify_encoding(audio, enclave, blob_id, timestamp_ms, signature)` | anyone | Checks the enclave's encoding signature over this Audio's facts and `blob_id`, then registers the blob as an encoding. Aborts `kagi::enclave::EInvalidSignature`, including when the signature is for audio with other facts, and `audio_encoding::audio_encoding::EEncodingAlreadyRegistered` if the blob is already registered. |

- **Two types, one package.** `AUDIO_VERIFIER` is the one-time witness that owns the kagi policy. `AudioVerifier` is the witness passed to `audio::new`; it is separate because Sui does not allow packing a one-time witness outside `init`.
- **Two signed payloads**, each `bcs(IntentMessage { intent, timestamp_ms, payload })`:
  - intent 0, `AudioPayload { channels: u8, bit_depth: u8, sample_rate_hz: u32, samples_per_channel: u64, pcm_digest: vector<u8> }`;
  - intent 1, `AudioEncodingPayload { channels, bit_depth, sample_rate_hz, samples_per_channel, pcm_digest, blob_id: u256 }`.

  The intent byte keeps either signature from verifying as the other. The encoding payload repeats the facts because they, not the digest alone, identify the Audio. `verify_encoding` reads them from the Audio it is given, so the caller passes only `blob_id`.
- **One measurement, both signatures.** The enclave's `/process_data` returns both. For new audio, in one PTB: `verify_audio`, `verify_encoding`, any records such as `recording_master::add_master`, then `audio::share`. For another file of existing audio, `verify_encoding` alone. Find out which applies by deriving the Audio's address with `audio::derive_address` and reading it; if two uploads race, the loser aborts with `EObjectAlreadyExists` and retries with `verify_encoding`.
- **The enclave type is fixed.** Both functions take `Enclave<AUDIO_VERIFIER>` only, so an enclave registered for another app can't sign. `verify_encoding` takes only this verifier's Audio.
- **No freshness check.** The facts describe the audio and a content-addressed Walrus blob, and don't go stale. Replaying a signature can only re-register an encoding or recreate audio that already exists, and both abort. `timestamp_ms` is the enclave's own clock time, provided by the host, so it's kept in the events only.
- **What it proves:** what the file contains, not who made it or who holds rights to it, and not that the blob is stored on Walrus. Anyone can point their own object at the `Audio<AudioVerifier>`. Authorship and duplicate detection belong in curation.
- **Audio from a retired image stays valid.** It was measured by an image that was trusted at the time. A consumer that wants to reject it has to track it by `enclave_id` and `policy_version` from the events.

## Operating the policy

1. Publish. The publisher receives the `EnclavePolicyCap<AUDIO_VERIFIER>`.
2. Build the enclave image (`../crates/audio-verifier-enclave`), record its PCRs, and call `kagi::enclave_policy::set_pcrs`.
3. Boot the enclave and fetch its attestation document. In one PTB, call `sui::nitro_attestation::load_nitro_attestation`, then `kagi::enclave::new`, then `share`, and transfer the `EnclaveCap`.
4. To rotate the image: in one PTB, call `set_pcrs` and `kagi::enclave::destroy` for every old enclave, then register the new one. Anyone can delete a leftover with `destroy_stale`.

## Events

- `AudioVerifiedEvent { enclave_id, policy_version, verified_at_ms, audio_id }` and `audio::audio::AudioCreatedEvent<AudioVerifier>` from `verify_audio`.
- `AudioEncodingVerifiedEvent { enclave_id, policy_version, verified_at_ms, audio_id, blob_id }` from each `verify_encoding` that registers a new blob, and `audio_encoding::audio_encoding::AudioEncodingRegisteredEvent<AudioVerifier, AudioVerifier>` the first time each blob is registered. This package is both the Audio's verifier and its encoding verifier, so consumers read its encodings with `audio_encoding::has_encoding<AudioVerifier, AudioVerifier>`.
- kagi emits the policy and enclave events.

## Layout and testing

```
sources/audio_verifier.move        the module
tests/audio_verifier_tests.move    unit tests, including the cross-language signing vector
```

The tests check that `bcs(AudioPayload)`, `bcs(AudioEncodingPayload)` and their intent messages match `../vectors/audio_intent.json` and `../vectors/audio_encoding_intent.json`, which the Rust crate generates. They verify each vector's signature, from the fixed seed `[7; 32]`, through kagi, and reject the same payload signed under the other intent, each signature against the other function, and an encoding signature against an Audio with other facts.

```sh
sui move build
sui move test --coverage
```

This package has not been audited.

## License

[Functional Source License 1.1, ALv2 Future License](../LICENSE.md)
(FSL-1.1-ALv2). Any use except a Competing Use is permitted, and each version
becomes Apache-2.0 two years after its release.
