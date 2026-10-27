use std::panic;

use libflac::{Error, ErrorStatus, Frame, Handler, StreamInfo, decode};

const FIXTURE: &[u8] =
    include_bytes!("../../audio-verifier/tests/fixtures/libflac_s16_stereo_44100.flac");

/// Records what libFLAC reports and can fail at a chosen frame.
#[derive(Default)]
struct Record {
    info: Option<StreamInfo>,
    frames: Vec<(u64, u64, usize, u32)>,
    fail_at: Option<u64>,
    panic_at: Option<u64>,
}

impl Handler for Record {
    type Error = &'static str;

    fn stream_info(&mut self, info: &StreamInfo) -> Result<(), Self::Error> {
        self.info = Some(*info);
        Ok(())
    }

    fn frame(&mut self, frame: &Frame<'_>) -> Result<(), Self::Error> {
        if self.panic_at == Some(frame.index) {
            panic!("handler panic");
        }
        if self.fail_at == Some(frame.index) {
            return Err("handler error");
        }
        assert_eq!(frame.samples.len(), frame.channels as usize);
        assert!(
            frame
                .samples
                .iter()
                .all(|channel| channel.len() == frame.block_size as usize)
        );
        self.frames.push((
            frame.index,
            frame.offset,
            frame.bytes.len(),
            frame.block_size,
        ));
        Ok(())
    }
}

#[test]
fn frames_tile_the_input_after_streaminfo() {
    let mut record = Record::default();
    let summary = decode(FIXTURE, &mut record).expect("decode");
    let info = record.info.expect("STREAMINFO");
    assert_eq!((info.channels, info.bits_per_sample), (2, 16));
    assert_eq!(summary.frames, record.frames.len() as u64);
    assert_eq!(summary.end, FIXTURE.len() as u64);

    let mut offset = 42;
    for (index, (frame, start, len, _)) in record.frames.iter().enumerate() {
        assert_eq!(*frame, index as u64);
        assert_eq!(*start, offset);
        assert_eq!(&FIXTURE[*start as usize..][..2], &[0xff, 0xf8]);
        offset += *len as u64;
    }
    assert_eq!(offset, FIXTURE.len() as u64);
    let samples: u64 = record.frames.iter().map(|frame| u64::from(frame.3)).sum();
    assert_eq!(samples, info.total_samples);
}

#[test]
fn handler_error_stops_decoding() {
    let mut record = Record {
        fail_at: Some(1),
        ..Record::default()
    };
    let error = decode(FIXTURE, &mut record).expect_err("handler error");
    assert!(matches!(error, Error::Handler("handler error")));
    assert_eq!(record.frames.len(), 1);
}

#[test]
fn handler_panic_is_resumed_on_the_caller() {
    let mut record = Record {
        panic_at: Some(0),
        ..Record::default()
    };
    let payload = panic::catch_unwind(panic::AssertUnwindSafe(|| decode(FIXTURE, &mut record)))
        .expect_err("handler panic");
    assert_eq!(payload.downcast_ref::<&str>(), Some(&"handler panic"));
}

#[test]
fn truncation_never_reaches_the_end_of_the_input() {
    let mut full = Record::default();
    decode(FIXTURE, &mut full).expect("decode");
    let (_, last_start, _, _) = *full.frames.last().expect("frames");

    // libFLAC reports some cuts as lost sync and stops quietly at others; either
    // way the last complete frame ends before the cut.
    let (mut errors, mut quiet) = (0, 0);
    for len in last_start as usize + 1..FIXTURE.len() {
        match decode(&FIXTURE[..len], &mut Record::default()) {
            Ok(summary) => {
                assert_eq!(summary.end, last_start, "cut at {len}");
                quiet += 1;
            }
            Err(Error::Stream { .. }) => errors += 1,
            Err(other) => panic!("cut at {len}: {other:?}"),
        }
    }
    eprintln!("{errors} cuts reported, {quiet} stopped quietly");
    assert!(quiet > 0, "no quiet stop exercised");
}

#[test]
fn corrupt_frame_is_a_stream_error() {
    let mut bytes = FIXTURE.to_vec();
    let middle = bytes.len() / 2;
    bytes[middle] ^= 0x01;
    let mut record = Record::default();
    let error = decode(&bytes, &mut record).expect_err("corrupt frame");
    assert!(
        matches!(
            error,
            Error::Stream {
                status: ErrorStatus::FrameCrcMismatch
                    | ErrorStatus::LostSync
                    | ErrorStatus::BadHeader,
                ..
            }
        ),
        "{error:?}"
    );
}

#[test]
fn trailing_garbage_is_lost_sync() {
    let mut bytes = FIXTURE.to_vec();
    bytes.extend_from_slice(b"TAG trailing bytes");
    let mut record = Record::default();
    let error = decode(&bytes, &mut record).expect_err("trailing bytes");
    assert!(
        matches!(
            error,
            Error::Stream {
                status: ErrorStatus::LostSync,
                ..
            }
        ),
        "{error:?}"
    );
}

#[test]
fn input_without_streaminfo_is_rejected() {
    let error = decode(b"not flac at all", &mut Record::default()).expect_err("not FLAC");
    assert!(!matches!(error, Error::Handler(_)), "{error:?}");
}
