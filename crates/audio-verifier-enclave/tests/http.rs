use std::path::Path;

use audio_verifier::measure;
use audio_verifier_enclave::{
    AUDIO_INTENT, DEFAULT_MAX_BODY_BYTES, ProcessDataResponse, app, max_body_bytes, server_config,
};
use axum::{
    Router,
    body::{Body, Bytes},
    http::{Request, Response, StatusCode},
};
use http_body_util::BodyExt as _;
use nautilus::{
    ConfigError, EnclaveIdentity, IntentMessage, NautilusContext, SignedResponse, intent_message,
    testing::FakeAttestor, verify_signed_response,
};
use tower::ServiceExt as _;

fn context() -> NautilusContext {
    NautilusContext::with_signer(
        EnclaveIdentity::from_seed([9u8; 32]),
        FakeAttestor::default(),
    )
}

fn fixture() -> Vec<u8> {
    std::fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../audio-verifier/tests/fixtures/s16_stereo_44100.flac"),
    )
    .expect("fixture")
}

fn post(body: Body) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri("/process_data")
        .body(body)
        .expect("request")
}

async fn json(response: Response<Body>) -> serde_json::Value {
    let bytes = response
        .into_body()
        .collect()
        .await
        .expect("body")
        .to_bytes();
    serde_json::from_slice(&bytes).expect("JSON")
}

#[tokio::test]
async fn process_data_signs_the_audio() {
    let ctx = context();
    let bytes = fixture();
    let response = app(ctx.clone(), server_config(DEFAULT_MAX_BODY_BYTES))
        .expect("config")
        .oneshot(post(Body::from(bytes.clone())))
        .await
        .expect("response");
    assert_eq!(response.status(), StatusCode::OK);
    let value = json(response).await;
    let body: ProcessDataResponse = serde_json::from_value(value.clone()).expect("shape");

    let measured = measure(&bytes).expect("fixture");
    assert_eq!(value["audio"]["intent"], AUDIO_INTENT.0);
    assert_eq!(value["audio"]["channels"], 2);
    assert_eq!(value["audio"]["bit_depth"], 16);
    assert_eq!(value["audio"]["sample_rate_hz"], 44_100);
    assert_eq!(value["audio"]["samples_per_channel"], "11025");
    assert!(value["audio"].get("format").is_none());
    assert_eq!(
        value
            .as_object()
            .expect("object")
            .keys()
            .collect::<Vec<_>>(),
        ["audio", "public_key"]
    );
    assert_eq!(
        body.audio.data.pcm_digest,
        hex::encode(&measured.pcm_digest)
    );
    assert_eq!(body.public_key, ctx.public_key_hex());
    assert!(body.audio.timestamp_ms > 1_700_000_000_000);

    let audio = SignedResponse {
        response: IntentMessage {
            intent: AUDIO_INTENT,
            timestamp_ms: body.audio.timestamp_ms,
            data: measured,
        },
        signature: body.audio.signature,
    };
    verify_signed_response(&ctx.public_key(), AUDIO_INTENT, &audio)
        .expect("audio signature verifies over the rebuilt intent message");
}

#[tokio::test]
async fn bcs_intent_message_body_is_rejected_unsigned() {
    let audio = measure(&fixture()).expect("fixture");
    let message = intent_message(AUDIO_INTENT, 1_750_000_000_000, &audio).expect("BCS");
    let response = app(context(), server_config(DEFAULT_MAX_BODY_BYTES))
        .expect("config")
        .oneshot(post(Body::from(message)))
        .await
        .expect("response");
    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let value = json(response).await;
    assert_eq!(value["error"], "application_error");
    assert!(value["message"].as_str().expect("message").contains("fLaC"));
    assert!(value.get("signature").is_none());
    assert!(!value.to_string().contains("signature"));
}

#[tokio::test]
async fn invalid_flac_is_rejected() {
    let mut bytes = fixture();
    bytes[30] ^= 1;
    let response = app(context(), server_config(DEFAULT_MAX_BODY_BYTES))
        .expect("config")
        .oneshot(post(Body::from(bytes)))
        .await
        .expect("response");
    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let value = json(response).await;
    assert!(value.get("signature").is_none());
}

#[tokio::test]
async fn body_limit_is_enforced() {
    let response = app(context(), server_config(4))
        .expect("config")
        .oneshot(post(Body::from(vec![0u8; 5])))
        .await
        .expect("response");
    assert_eq!(response.status(), StatusCode::PAYLOAD_TOO_LARGE);
}

#[tokio::test]
async fn second_upload_is_refused_while_one_is_in_flight() {
    let router: Router = app(context(), server_config(DEFAULT_MAX_BODY_BYTES)).expect("config");
    let (sender, receiver) = tokio::sync::mpsc::channel::<Result<Bytes, std::io::Error>>(1);
    let stream = tokio_stream(receiver);
    let mut first = Box::pin(router.clone().oneshot(post(Body::from_stream(stream))));
    assert!(futures_util::poll!(&mut first).is_pending());

    let response = router
        .clone()
        .oneshot(post(Body::from(fixture())))
        .await
        .expect("response");
    assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(json(response).await["error"], "overloaded");

    sender.send(Ok(Bytes::from(fixture()))).await.expect("send");
    drop(sender);
    let response = first.await.expect("response");
    assert_eq!(response.status(), StatusCode::OK);

    let response = router
        .oneshot(post(Body::from(fixture())))
        .await
        .expect("response");
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn health_check_reports_the_injected_identity() {
    let ctx = context();
    let response = app(ctx.clone(), server_config(DEFAULT_MAX_BODY_BYTES))
        .expect("config")
        .oneshot(
            Request::builder()
                .uri("/health_check")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(json(response).await["pk"], ctx.public_key_hex());
}

#[test]
fn max_body_bytes_resolution() {
    assert_eq!(
        max_body_bytes(&[], None).expect("default"),
        256 * 1024 * 1024
    );
    assert_eq!(max_body_bytes(&[], Some("1024".into())).expect("env"), 1024);
    let args = ["--max-body-bytes".to_owned(), "2048".to_owned()];
    assert_eq!(
        max_body_bytes(&args, Some("1024".into())).expect("flag"),
        2048
    );
    assert!(max_body_bytes(&["--dev".to_owned()], None).is_err());
    assert!(max_body_bytes(&[], Some("lots".into())).is_err());
}

fn tokio_stream(
    mut receiver: tokio::sync::mpsc::Receiver<Result<Bytes, std::io::Error>>,
) -> impl futures_util::Stream<Item = Result<Bytes, std::io::Error>> {
    futures_util::stream::poll_fn(move |cx| receiver.poll_recv(cx))
}

#[test]
fn server_config_serves_one_upload_within_the_request_deadline() {
    let config = server_config(DEFAULT_MAX_BODY_BYTES);
    assert_eq!(config.max_concurrent, 1);
    assert!(config.body_read_timeout < config.request_timeout);
    assert_eq!(config.max_connections, 256);
    assert!(!config.allow_unnonced_attestation);
    config.validate().expect("valid");
}

#[test]
fn zero_body_limit_is_a_config_error() {
    assert_eq!(
        app(context(), server_config(0)).err(),
        Some(ConfigError::Zero("max_body_bytes"))
    );
}

#[tokio::test]
async fn excessive_decoded_work_is_rejected_unsigned() {
    let mut bytes = fixture();
    // Preserve format fields and replace the 36-bit sample count with its maximum.
    bytes[21] |= 0x0f;
    bytes[22..26].fill(0xff);
    let response = app(context(), server_config(DEFAULT_MAX_BODY_BYTES))
        .expect("config")
        .oneshot(post(Body::from(bytes)))
        .await
        .expect("response");
    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let value = json(response).await;
    assert!(value["message"].as_str().unwrap().contains("decoded PCM"));
    assert!(value.get("signature").is_none());
}

#[test]
fn ambiguous_or_zero_body_limits_are_rejected() {
    assert!(max_body_bytes(&[], Some("0".into())).is_err());
    let args = ["--max-body-bytes", "1024", "--max-body-bytes", "2048"].map(String::from);
    assert!(max_body_bytes(&args, None).is_err());
}
