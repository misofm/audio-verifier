# Nautilus Audio Verifier

Measures FLAC audio inside a Nautilus enclave on AWS Nitro and verifies the signed results on Sui. It checks the decoded audio, computes a BLAKE3 PCM digest, and signs the audio's channels, bit depth, sample rate, sample count and PCM digest. It signs nothing about the file itself.

The [`audio_verifier`](https://github.com/misofm/framework/tree/main/audio_verifier) Move package, in `misofm/framework`, verifies the signature and creates the canonical, frozen `Audio<AudioVerifier>` for use by other applications. Files are not on-chain: anyone can check a file against an Audio by decoding it and comparing the BLAKE3 digest of its PCM with `pcm_digest`. This verifies technical properties of the audio; it does not establish authorship or ownership.

## Supported input

The service accepts canonical FLAC files with:

- STREAMINFO as the only metadata block, with a nonzero MD5 and sample count.
- 8-, 16-, or 24-bit samples.
- Sample rates of 44.1, 48, 88.2, 96, 176.4, 192, 352.8, or 384 kHz.
- Metadata and frame contents that agree with the decoded audio.

Uploads default to a 256 MiB limit. Decoded PCM is limited to 4 GiB across all channels. Non-FLAC files and invalid audio are rejected without a signature.

## Run locally

Install Rust using the version in `rust-toolchain.toml` and a C compiler, then start the development service:

```sh
cargo run --locked -p audio-verifier-enclave --features dev
```

This listens on `127.0.0.1:3000` with a fixed development identity. It is for local testing only.

Prepare a copy of your FLAC file by removing tags and other metadata with `metaflac` (from the FLAC tools), then upload it:

```sh
cp audio.flac canonical.flac
metaflac --remove-all --dont-use-padding canonical.flac
curl --fail-with-body http://127.0.0.1:3000/process_data \
  -H 'Content-Type: application/octet-stream' \
  --data-binary @canonical.flac
```

The JSON response contains `audio` (the facts, with its `intent` 0, `timestamp_ms` and `signature`) and `public_key`.

Set the upload limit with `--max-body-bytes` after Cargo's `--`, or with `AUDIO_VERIFIER_MAX_BODY_BYTES`. The service processes one request at a time; concurrent uploads receive HTTP 429. Invalid audio receives HTTP 422 and oversized uploads receive HTTP 413.

## Use on Sui

Publish the [`audio_verifier` Move package](https://github.com/misofm/framework/tree/main/audio_verifier), configure its enclave policy, and register an attested enclave. For new audio, pass `audio`'s fields, timestamp and signature to `audio_verifier::audio_verifier::verify_audio`, which checks the signature against the registered enclave, then freeze the returned Audio with `audio.freeze()` in the same transaction.

See its [README](https://github.com/misofm/framework/tree/main/audio_verifier) for the API and policy setup.

## Run in an enclave

The production build requires AWS Nitro's NSM device and serves HTTP on vsock port 3000. The enclave image requires an x86-64 CPU with AVX-512 and GFNI support.

With Docker available, build the image; on a configured Nitro host with `nitro-cli`, run it:

```sh
make eif
make run-eif
```

The image is written to `out/nitro.eif`. The default enclave allocation is four vCPUs and 6144 MiB of memory. Bridge clients to vsock port 3000 through the parent instance, and register the image's PCR measurements through the Move enclave policy before using its signatures on-chain. Never enable the `dev` feature in a production image.

## Test

```sh
cargo test --locked --workspace
```

Installing `ffmpeg` enables additional decoder cross-checks. The Move package's tests live with it in `misofm/framework`.

## License

[Functional Source License 1.1, ALv2 Future License](LICENSE.md). Vendored libFLAC is [BSD-3-Clause licensed](crates/libflac/flac-1.5.0/COPYING.Xiph); its notice must accompany binary distributions.
