# audio_verifier (Move package)

`verify` returns an [`audio::Audio<AudioVerifier>`](https://github.com/misofm/audio), lossless audio facts measured inside a Nautilus (AWS Nitro) enclave and checked on Sui. `AudioVerifier` is this package's witness, and only a signature from a registered `Enclave<AUDIO_VERIFIER>` can produce one. Consumers that trust this enclave pin `Audio<audio_verifier::audio_verifier::AudioVerifier>`.

There is no docs.miso.fm page yet, so the facts are below.

```toml
audio_verifier = { git = "https://github.com/misofm/audio-verifier.git", subdir = "move", rev = "<commit>" }
```

## API

| Function | Who may call | What it does |
|---|---|---|
| `init` | publish | Creates the one `kagi::EnclavePolicy<AUDIO_VERIFIER>`, with its PCRs unset, shares it and sends its `EnclavePolicyCap` to the publisher. |
| `verify(enclave, format, channels, bit_depth, sample_rate_hz, samples_per_channel, pcm_digest, blob_id, timestamp_ms, signature)` | anyone | Checks the enclave's signature over the facts and returns `Audio<AudioVerifier>` with an unencrypted `ori::data::WalrusBlob`. Aborts `kagi::enclave::EInvalidSignature`, or with an `audio::audio` error if the facts are malformed. |

- **Two types, one package.** `AUDIO_VERIFIER` is the one-time witness that owns the kagi policy. `AudioVerifier` is the witness passed to `audio::new`; it is separate because Sui does not allow packing a one-time witness outside `init`.
- **The signed payload is `AudioVerificationPayload`:** `bcs(IntentMessage { intent: 0, timestamp_ms, payload })`, with the fields `format: String`, `channels: u8`, `bit_depth: u8`, `sample_rate_hz: u32`, `samples_per_channel: u64`, `pcm_digest: vector<u8>`, `blob_id: u256`, in that order. The caller passes them from the enclave's response.
- **The enclave type is fixed.** `verify` takes `Enclave<AUDIO_VERIFIER>` only, so an enclave registered for another app can't sign.
- **No freshness check.** The facts describe a content-addressed Walrus blob and don't go stale. Replaying a signature rebuilds the same value, and `Audio` can be copied anyway. `timestamp_ms` is the enclave's own clock time, provided by the host, so it's kept in the event only.
- **What it proves:** what the file contains, not who made it or who holds rights to it. Anyone can attach the `Audio<AudioVerifier>` to their own object. Authorship and duplicate detection belong in curation.
- **Values from a retired image stay valid.** They were measured by an image that was trusted at the time. A consumer that wants to reject one has to track it by `enclave_id` and `policy_version` from the event.

## Operating the policy

1. Publish. The publisher receives the `EnclavePolicyCap<AUDIO_VERIFIER>`.
2. Build the enclave image (`../crates/audio-verifier-enclave`), record its PCRs, and call `kagi::enclave_policy::set_pcrs`.
3. Boot the enclave and fetch its attestation document. In one PTB, call `sui::nitro_attestation::load_nitro_attestation`, then `kagi::enclave::new`, then `share`, and transfer the `EnclaveCap`.
4. To rotate the image: in one PTB, call `set_pcrs` and `kagi::enclave::destroy` for every old enclave, then register the new one. Anyone can delete a leftover with `destroy_stale`.

## Events

- `AudioVerifiedEvent { enclave_id, policy_version, verified_at_ms, pcm_digest, blob_id }`, once per `verify`.
- `audio::audio::AudioCreatedEvent<AudioVerifier>`, once per `verify`, with every audio fact including `format`.
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
