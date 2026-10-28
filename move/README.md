# audio_verifier (Move package)

`verify_audio` creates an [`audio::Audio<AudioVerifier>`](https://github.com/misofm/audio), the canonical record of lossless audio measured inside a Nautilus (AWS Nitro) enclave and checked on Sui, and `verify_encoding` attests another file of an existing one. `AudioVerifier` is this package's witness, and only a signature from a registered `Enclave<AUDIO_VERIFIER>` can produce either. Consumers that trust this enclave pin `Audio<audio_verifier::audio_verifier::AudioVerifier>`.

There is no docs.miso.fm page yet, so the facts are below.

```toml
audio_verifier = { git = "https://github.com/misofm/audio-verifier.git", subdir = "move", rev = "<commit>" }
```

## API

| Function | Who may call | What it does |
|---|---|---|
| `init` | publish | Creates the one `kagi::EnclavePolicy<AUDIO_VERIFIER>`, with its PCRs unset, shares it and sends its `EnclavePolicyCap` to the publisher. |
| `verify_audio(registry, enclave, format, channels, bit_depth, sample_rate_hz, samples_per_channel, pcm_digest, blob_id, timestamp_ms, signature)` | anyone | Checks the enclave's signature, creates the `Audio<AudioVerifier>` from the registry with the blob as its first encoding, and returns it unshared: attach records to it, then call `audio::share`. Aborts `kagi::enclave::EInvalidSignature`, `audio::EAudioExists` if the audio is already registered, or another `audio::audio` error if the facts are malformed. |
| `verify_encoding(audio, enclave, format, channels, bit_depth, sample_rate_hz, samples_per_channel, pcm_digest, blob_id, timestamp_ms, signature)` | anyone | For audio that already exists: checks that the signed facts are this Audio's (`EWrongAudio`, 0) and the signature, then registers the blob as an encoding. An encoding already registered is left as is, and the verification is still emitted. |

- **Two types, one package.** `AUDIO_VERIFIER` is the one-time witness that owns the kagi policy. `AudioVerifier` is the witness passed to `audio::new`; it is separate because Sui does not allow packing a one-time witness outside `init`.
- **Use the right path.** Derive the Audio's address with `audio::derive_address` and read it: if it doesn't exist call `verify_audio`, otherwise `verify_encoding`. If two uploads race, the loser aborts `EAudioExists` and retries with `verify_encoding`.
- **The signed payload is `AudioVerificationPayload`:** `bcs(IntentMessage { intent: 0, timestamp_ms, payload })`, with the fields `format: String`, `channels: u8`, `bit_depth: u8`, `sample_rate_hz: u32`, `samples_per_channel: u64`, `pcm_digest: vector<u8>`, `blob_id: u256`, in that order. The caller passes them from the enclave's response. The function parameters and the Audio use this same order. `format` is signed but not stored.
- **The enclave type is fixed.** Both functions take `Enclave<AUDIO_VERIFIER>` only, so an enclave registered for another app can't sign.
- **No freshness check.** The facts describe a content-addressed Walrus blob and don't go stale. Replaying a signature can only re-register an encoding that is already there. `timestamp_ms` is the enclave's own clock time, provided by the host, so it's kept in the event only.
- **What it proves:** what the file contains, not who made it or who holds rights to it. Anyone can point their own object at the `Audio<AudioVerifier>`. Authorship and duplicate detection belong in curation.
- **Audio from a retired image stays valid.** They were measured by an image that was trusted at the time. A consumer that wants to reject one has to track it by `enclave_id` and `policy_version` from the event.

## Operating the policy

1. Publish. The publisher receives the `EnclavePolicyCap<AUDIO_VERIFIER>`.
2. Build the enclave image (`../crates/audio-verifier-enclave`), record its PCRs, and call `kagi::enclave_policy::set_pcrs`.
3. Boot the enclave and fetch its attestation document. In one PTB, call `sui::nitro_attestation::load_nitro_attestation`, then `kagi::enclave::new`, then `share`, and transfer the `EnclaveCap`.
4. To rotate the image: in one PTB, call `set_pcrs` and `kagi::enclave::destroy` for every old enclave, then register the new one. Anyone can delete a leftover with `destroy_stale`.

## Events

- `AudioVerifiedEvent { enclave_id, policy_version, verified_at_ms, audio_id, blob_id }`, once per `verify_audio` or `verify_encoding`.
- `audio::audio::AudioCreatedEvent<AudioVerifier>` from `verify_audio`, with every audio fact.
- `audio::audio::AudioEncodingRegisteredEvent<AudioVerifier, AudioVerifier>` the first time each blob is attested. This package is both the Audio's verifier and its encoding verifier, so it passes `AudioVerifier` as both, and consumers read its encodings with `has_encoding<AudioVerifier, AudioVerifier>`.
- kagi emits the policy and enclave events.

## Layout and testing

```
sources/audio_verifier.move        the module
tests/audio_verifier_tests.move    unit tests, including the cross-language signing vector
```

The tests check that `bcs(AudioVerificationPayload)` and the intent message match `../vectors/audio_intent.json`, which the Rust crate generates. They also verify that vector's signature, from the fixed seed `[7; 32]`, through kagi, and reject a signature by the same key over intent 1.

```sh
sui move build
sui move test --coverage
```

This package has not been audited.

## License

[Functional Source License 1.1, ALv2 Future License](../LICENSE.md)
(FSL-1.1-ALv2). Any use except a Competing Use is permitted, and each version
becomes Apache-2.0 two years after its release.
