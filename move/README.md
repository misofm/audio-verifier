# audio_verifier (Move package)

`verify_audio` creates an [`audio::Audio<AudioVerifier>`](https://github.com/misofm/framework/tree/main/audio), the canonical record of lossless audio measured inside a Nautilus (AWS Nitro) enclave and checked on Sui. `AudioVerifier` is this package's witness, and only a signature from a registered `Enclave<AUDIO_VERIFIER>` can produce one. Consumers that trust this enclave pin `Audio<audio_verifier::audio_verifier::AudioVerifier>`.

There is no docs.miso.fm page yet, so the facts are below.

```toml
audio_verifier = { git = "https://github.com/misofm/audio-verifier.git", subdir = "move", rev = "<commit>" }
```

## API

| Function | Who may call | What it does |
|---|---|---|
| `init` | publish | Creates the one `kagi::EnclavePolicy<AUDIO_VERIFIER>`, with its PCRs unset, shares it and sends its `EnclavePolicyCap` to the publisher. |
| `verify_audio(registry, enclave, channels, bit_depth, sample_rate_hz, samples_per_channel, pcm_digest, timestamp_ms, signature)` | anyone | Checks the enclave's audio signature, creates the `Audio<AudioVerifier>` from the registry and returns it unfrozen: point records at it, then call `audio.freeze()`. Aborts `kagi::enclave::EInvalidSignature`, `derived_object::EObjectAlreadyExists` if the audio is already registered, or another `audio::audio` error if the facts are malformed. |

- **Two types, one package.** `AUDIO_VERIFIER` is the one-time witness that owns the kagi policy. `AudioVerifier` is the witness passed to `audio::new`; it is separate because Sui does not allow packing a one-time witness outside `init`.
- **One signed payload**, `bcs(IntentMessage { intent: 0, timestamp_ms, payload })` with `AudioPayload { channels: u8, bit_depth: u8, sample_rate_hz: u32, samples_per_channel: u64, pcm_digest: vector<u8> }`. A signature under any other intent does not verify.
- **One measurement, one Audio.** The enclave's `/process_data` returns the signed facts. For new audio, in one PTB: `verify_audio`, any records such as `recording_master::add_master`, then `audio.freeze()`. Before uploading, derive the Audio's address with `audio::derive_address` and read it; if it exists, the audio is already registered. If two uploads race, the loser aborts with `EObjectAlreadyExists`.
- **Files are not on-chain.** The enclave decodes the uploaded file to measure it but signs nothing about the file. Anyone can check a file against an Audio by decoding it and comparing the BLAKE3 digest of its PCM with `pcm_digest`.
- **The enclave type is fixed.** `verify_audio` takes `Enclave<AUDIO_VERIFIER>` only, so an enclave registered for another app can't sign.
- **No freshness check.** The facts describe the audio and don't go stale. Replaying a signature can only recreate audio that already exists, which aborts. `timestamp_ms` is the enclave's own clock time, provided by the host, so it's kept in the events only.
- **What it proves:** what the audio is, not who made it or who holds rights to it. Anyone can point their own object at the `Audio<AudioVerifier>`. Authorship and duplicate detection belong in curation.
- **Audio from a retired image stays valid.** It was measured by an image that was trusted at the time. A consumer that wants to reject it has to track it by `enclave_id` and `policy_version` from the events.

## Operating the policy

1. Publish. The publisher receives the `EnclavePolicyCap<AUDIO_VERIFIER>`.
2. Build the enclave image (`../crates/audio-verifier-enclave`), record its PCRs, and call `kagi::enclave_policy::set_pcrs`.
3. Boot the enclave and fetch its attestation document. In one PTB, call `sui::nitro_attestation::load_nitro_attestation`, then `kagi::enclave::new`, then `share`, and transfer the `EnclaveCap`.
4. To rotate the image: in one PTB, call `set_pcrs` and `kagi::enclave::destroy` for every old enclave, then register the new one. Anyone can delete a leftover with `destroy_stale`.

## Events

- `AudioVerifiedEvent { enclave_id, policy_version, verified_at_ms, audio_id }` and `audio::audio::AudioCreatedEvent<AudioVerifier>` from `verify_audio`.
- kagi emits the policy and enclave events.

## Layout and testing

```
sources/audio_verifier.move        the module
tests/audio_verifier_tests.move    unit tests, including the cross-language signing vector
```

The tests check that `bcs(AudioPayload)` and its intent message match `../vectors/audio_intent.json`, which the Rust crate generates. They verify the vector's signature, from the fixed seed `[7; 32]`, through kagi, reject the same payload signed under another intent, and freeze the Audio.

```sh
sui move build
sui move test --coverage
```

This package has not been audited.

## License

[Functional Source License 1.1, ALv2 Future License](../LICENSE.md)
(FSL-1.1-ALv2). Any use except a Competing Use is permitted, and each version
becomes Apache-2.0 two years after its release.
