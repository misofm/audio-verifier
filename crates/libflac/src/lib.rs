//! A safe callback interface to libFLAC 1.5.0's stream decoder, compiled from the
//! vendored release sources in `flac-1.5.0/`.
//!
//! [`decode`] runs libFLAC over an in-memory stream and hands each decoded frame,
//! with its exact byte range, to a [`Handler`]. Unlike libFLAC's default
//! behaviour, decoding is strict: the first error libFLAC reports ends it, so
//! libFLAC never resynchronises, skips bytes or fills missing frames with silence
//! on the handler's behalf.

mod ffi;

use std::any::Any;
use std::cell::{Cell, RefCell};
use std::ffi::{c_int, c_void};
use std::fmt;
use std::panic::{self, AssertUnwindSafe};
use std::{ptr, slice};

/// The most channels a FLAC frame can have.
const MAX_CHANNELS: usize = 8;

/// STREAMINFO as libFLAC parsed it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StreamInfo {
    pub min_block_size: u32,
    pub max_block_size: u32,
    pub min_frame_size: u32,
    pub max_frame_size: u32,
    pub sample_rate: u32,
    pub channels: u32,
    pub bits_per_sample: u32,
    pub total_samples: u64,
    pub md5: [u8; 16],
}

/// A decoded frame.
#[derive(Debug)]
pub struct Frame<'a> {
    /// Zero-based position of the frame in the stream.
    pub index: u64,
    /// Samples per channel.
    pub block_size: u32,
    pub sample_rate: u32,
    pub channels: u32,
    pub bits_per_sample: u32,
    /// Byte offset of the frame in the input.
    pub offset: u64,
    /// The frame's bytes, from the start of its header through its CRC-16: the
    /// input from the end of the previous frame, or of the metadata, to where
    /// libFLAC finished this one.
    pub bytes: &'a [u8],
    /// Decoded samples, one slice of `block_size` samples per channel.
    pub samples: &'a [&'a [i32]],
}

/// Receives what [`decode`] decodes. An error from either method ends decoding.
pub trait Handler {
    type Error;

    /// Called once, with the stream's STREAMINFO, before any frame.
    fn stream_info(&mut self, info: &StreamInfo) -> Result<(), Self::Error>;

    /// Called for each frame, in order.
    fn frame(&mut self, frame: &Frame<'_>) -> Result<(), Self::Error>;
}

/// An error libFLAC reported through its error callback.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorStatus {
    LostSync,
    BadHeader,
    FrameCrcMismatch,
    UnparseableStream,
    BadMetadata,
    OutOfBounds,
    MissingFrame,
    Unknown(c_int),
}

impl ErrorStatus {
    fn from_raw(status: c_int) -> Self {
        match status {
            0 => Self::LostSync,
            1 => Self::BadHeader,
            2 => Self::FrameCrcMismatch,
            3 => Self::UnparseableStream,
            4 => Self::BadMetadata,
            5 => Self::OutOfBounds,
            6 => Self::MissingFrame,
            other => Self::Unknown(other),
        }
    }
}

impl fmt::Display for ErrorStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::LostSync => f.write_str("lost sync"),
            Self::BadHeader => f.write_str("corrupt frame header"),
            Self::FrameCrcMismatch => f.write_str("frame CRC-16 mismatch"),
            Self::UnparseableStream => f.write_str("reserved field in use"),
            Self::BadMetadata => f.write_str("corrupt metadata"),
            Self::OutOfBounds => f.write_str("decoded samples exceed the bit depth"),
            Self::MissingFrame => f.write_str("missing frame"),
            Self::Unknown(status) => write!(f, "error status {status}"),
        }
    }
}

/// Why [`decode`] stopped.
#[derive(Debug, thiserror::Error)]
pub enum Error<E> {
    /// The handler returned an error.
    #[error("{0}")]
    Handler(E),
    /// libFLAC reported an error in the stream.
    #[error("libFLAC: {status} in frame {frame} at byte {offset}")]
    Stream {
        status: ErrorStatus,
        /// Index of the frame being decoded.
        frame: u64,
        /// End of the last complete frame, or of the metadata.
        offset: u64,
    },
    /// libFLAC finished the metadata without reporting STREAMINFO.
    #[error("libFLAC found no STREAMINFO")]
    MissingStreamInfo,
    /// libFLAC reported STREAMINFO more than once.
    #[error("libFLAC found more than one STREAMINFO")]
    RepeatedStreamInfo,
    /// libFLAC could not report a byte position inside the input.
    #[error("libFLAC could not report its decode position")]
    DecodePosition,
    /// libFLAC could not allocate or initialize a decoder.
    #[error("libFLAC decoder initialization failed with status {0}")]
    Init(c_int),
    /// libFLAC stopped in a state other than end of stream.
    #[error("libFLAC decoder stopped in state {}", state_name(*.0))]
    State(c_int),
}

/// How a successful [`decode`] ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Summary {
    /// Frames decoded.
    pub frames: u64,
    /// Byte offset just past the last frame, or past the metadata if there were
    /// no frames. Input after it was not decoded: libFLAC stops quietly at a
    /// truncated final frame, so callers that require the whole input to be
    /// frames must compare this with its length.
    pub end: u64,
}

/// Decodes `input` with libFLAC, passing STREAMINFO and every frame to `handler`.
///
/// A panic in `handler` is resumed on the caller's thread after libFLAC returns.
pub fn decode<H: Handler>(input: &[u8], handler: &mut H) -> Result<Summary, Error<H::Error>> {
    let session = Session {
        input,
        read: Cell::new(0),
        handler: RefCell::new(handler),
        stream_info: Cell::new(false),
        frames: Cell::new(0),
        end: Cell::new(0),
        failure: RefCell::new(None),
        panic: Cell::new(None),
    };
    // Declared after `session`, so the decoder is deleted before the session it
    // points to is dropped.
    let decoder = Decoder::new()?;
    let client_data = ptr::from_ref(&session).cast_mut().cast::<c_void>();
    // SAFETY: `decoder` is a fresh decoder. The callbacks are instantiated for
    // this `H` and cast `client_data` back to `Session<H>`; `session` outlives
    // the decoder and is only accessed through shared references.
    let status = unsafe {
        ffi::FLAC__stream_decoder_init_stream(
            decoder.0,
            Some(read::<H>),
            None,
            Some(tell::<H>),
            None,
            None,
            Some(write::<H>),
            Some(metadata::<H>),
            Some(error::<H>),
            client_data,
        )
    };
    if status != ffi::INIT_STATUS_OK {
        return Err(Error::Init(status));
    }

    // SAFETY: `decoder` was initialized above.
    unsafe { ffi::FLAC__stream_decoder_process_until_end_of_metadata(decoder.0) };
    session.finish_step()?;
    if !session.stream_info.get() {
        return Err(Error::MissingStreamInfo);
    }
    session
        .end
        .set(decoder.position().ok_or(Error::DecodePosition)?);

    // SAFETY: as above.
    unsafe { ffi::FLAC__stream_decoder_process_until_end_of_stream(decoder.0) };
    session.finish_step()?;
    // SAFETY: as above.
    let state = unsafe { ffi::FLAC__stream_decoder_get_state(decoder.0) };
    if state != ffi::STATE_END_OF_STREAM {
        return Err(Error::State(state));
    }
    Ok(Summary {
        frames: session.frames.get(),
        end: session.end.get(),
    })
}

/// Owns a libFLAC decoder.
struct Decoder(*mut ffi::StreamDecoder);

impl Decoder {
    fn new<E>() -> Result<Self, Error<E>> {
        // SAFETY: no preconditions; a null result means allocation failed.
        let decoder = unsafe { ffi::FLAC__stream_decoder_new() };
        if decoder.is_null() {
            return Err(Error::State(ffi::STATE_MEMORY_ALLOCATION_ERROR));
        }
        Ok(Self(decoder))
    }

    fn position(&self) -> Option<u64> {
        position(self.0)
    }
}

impl Drop for Decoder {
    fn drop(&mut self) {
        // SAFETY: `self.0` came from `FLAC__stream_decoder_new` and is deleted once.
        unsafe { ffi::FLAC__stream_decoder_delete(self.0) };
    }
}

fn position(decoder: *const ffi::StreamDecoder) -> Option<u64> {
    let mut position = 0;
    // SAFETY: `decoder` is a live, initialized decoder and `position` is a valid
    // out-pointer. libFLAC calls `tell`, which only reads `Session::read`.
    let ok = unsafe { ffi::FLAC__stream_decoder_get_decode_position(decoder, &mut position) };
    (ok != 0).then_some(position)
}

/// State shared with the callbacks, which see it only through `&Session`.
struct Session<'a, H: Handler> {
    input: &'a [u8],
    /// Bytes handed to libFLAC so far.
    read: Cell<usize>,
    handler: RefCell<&'a mut H>,
    stream_info: Cell<bool>,
    frames: Cell<u64>,
    /// End of the last complete frame, or of the metadata.
    end: Cell<u64>,
    /// The first failure; once set, every later callback aborts.
    failure: RefCell<Option<Error<H::Error>>>,
    panic: Cell<Option<Box<dyn Any + Send>>>,
}

impl<H: Handler> Session<'_, H> {
    fn failed(&self) -> bool {
        self.failure.borrow().is_some()
    }

    fn fail(&self, error: Error<H::Error>) {
        let mut failure = self.failure.borrow_mut();
        if failure.is_none() {
            *failure = Some(error);
        }
    }

    /// Runs a handler method, recording an error or a panic as the failure.
    fn call(&self, f: impl FnOnce(&mut H) -> Result<(), H::Error>) -> bool {
        let mut handler = self.handler.borrow_mut();
        match panic::catch_unwind(AssertUnwindSafe(|| f(&mut handler))) {
            Ok(Ok(())) => true,
            Ok(Err(error)) => {
                drop(handler);
                self.fail(Error::Handler(error));
                false
            }
            Err(payload) => {
                drop(handler);
                self.panic.set(Some(payload));
                // Any recorded failure stops decoding; the panic takes precedence.
                self.fail(Error::State(ffi::STATE_ABORTED));
                false
            }
        }
    }

    /// Resumes a handler panic, or returns the first failure, after libFLAC returns.
    fn finish_step(&self) -> Result<(), Error<H::Error>> {
        if let Some(payload) = self.panic.take() {
            panic::resume_unwind(payload);
        }
        match self.failure.borrow_mut().take() {
            Some(error) => Err(error),
            None => Ok(()),
        }
    }
}

/// # Safety
///
/// `client_data` must be the `Session<H>` passed to `decode`'s `init_stream`.
unsafe fn session<'s, 'a, H: Handler>(client_data: *mut c_void) -> &'s Session<'a, H> {
    // SAFETY: guaranteed by the caller; the session outlives the decoder.
    unsafe { &*client_data.cast::<Session<'a, H>>() }
}

unsafe extern "C" fn read<H: Handler>(
    _decoder: *const ffi::StreamDecoder,
    buffer: *mut u8,
    bytes: *mut usize,
    client_data: *mut c_void,
) -> c_int {
    // SAFETY: libFLAC passes back the client data given to `init_stream`.
    let session = unsafe { session::<H>(client_data) };
    if session.failed() {
        return ffi::READ_STATUS_ABORT;
    }
    let remaining = &session.input[session.read.get()..];
    // SAFETY: libFLAC passes a valid in/out byte count.
    let capacity = unsafe { *bytes };
    let count = remaining.len().min(capacity);
    // SAFETY: `buffer` has room for `capacity >= count` bytes, and it does not
    // overlap the input slice.
    unsafe {
        ptr::copy_nonoverlapping(remaining.as_ptr(), buffer, count);
        *bytes = count;
    }
    session.read.set(session.read.get() + count);
    if count == 0 {
        ffi::READ_STATUS_END_OF_STREAM
    } else {
        ffi::READ_STATUS_CONTINUE
    }
}

unsafe extern "C" fn tell<H: Handler>(
    _decoder: *const ffi::StreamDecoder,
    offset: *mut u64,
    client_data: *mut c_void,
) -> c_int {
    // SAFETY: libFLAC passes back the client data given to `init_stream`.
    let session = unsafe { session::<H>(client_data) };
    // SAFETY: libFLAC passes a valid out-pointer.
    unsafe { *offset = session.read.get() as u64 };
    ffi::TELL_STATUS_OK
}

unsafe extern "C" fn metadata<H: Handler>(
    _decoder: *const ffi::StreamDecoder,
    block: *const ffi::StreamMetadata,
    client_data: *mut c_void,
) {
    // SAFETY: libFLAC passes back the client data given to `init_stream`.
    let session = unsafe { session::<H>(client_data) };
    // SAFETY: libFLAC passes a valid metadata block for the callback's duration.
    let block = unsafe { &*block };
    if session.failed() || block.type_ != ffi::METADATA_TYPE_STREAMINFO {
        return;
    }
    if session.stream_info.replace(true) {
        session.fail(Error::RepeatedStreamInfo);
        return;
    }
    let raw = &block.stream_info;
    let info = StreamInfo {
        min_block_size: raw.min_blocksize,
        max_block_size: raw.max_blocksize,
        min_frame_size: raw.min_framesize,
        max_frame_size: raw.max_framesize,
        sample_rate: raw.sample_rate,
        channels: raw.channels,
        bits_per_sample: raw.bits_per_sample,
        total_samples: raw.total_samples,
        md5: raw.md5sum,
    };
    session.call(|handler| handler.stream_info(&info));
}

unsafe extern "C" fn write<H: Handler>(
    decoder: *const ffi::StreamDecoder,
    header: *const ffi::FrameHeader,
    buffer: *const *const i32,
    client_data: *mut c_void,
) -> c_int {
    // SAFETY: libFLAC passes back the client data given to `init_stream`.
    let session = unsafe { session::<H>(client_data) };
    if session.failed() {
        return ffi::WRITE_STATUS_ABORT;
    }
    // The whole frame, CRC-16 included, has been consumed by now.
    let Some(end) = position(decoder) else {
        session.fail(Error::DecodePosition);
        return ffi::WRITE_STATUS_ABORT;
    };
    let start = session.end.get();
    let Some(bytes) = usize::try_from(start)
        .ok()
        .zip(usize::try_from(end).ok())
        .and_then(|(start, end)| session.input.get(start..end))
    else {
        session.fail(Error::DecodePosition);
        return ffi::WRITE_STATUS_ABORT;
    };

    // SAFETY: the write callback's frame pointer is valid for its duration, and
    // `FrameHeader` is a prefix of `FLAC__Frame` (checked in layout.c).
    let header = unsafe { &*header };
    let channels = header.channels as usize;
    let block_size = header.blocksize as usize;
    let mut samples: [&[i32]; MAX_CHANNELS] = [&[]; MAX_CHANNELS];
    for (channel, slot) in samples.iter_mut().enumerate().take(channels) {
        // SAFETY: libFLAC passes `channels <= 8` channel pointers, each to
        // `blocksize` samples valid for the callback's duration.
        *slot = unsafe { slice::from_raw_parts(*buffer.add(channel), block_size) };
    }
    let frame = Frame {
        index: session.frames.get(),
        block_size: header.blocksize,
        sample_rate: header.sample_rate,
        channels: header.channels,
        bits_per_sample: header.bits_per_sample,
        offset: start,
        bytes,
        samples: &samples[..channels.min(MAX_CHANNELS)],
    };
    if !session.call(|handler| handler.frame(&frame)) {
        return ffi::WRITE_STATUS_ABORT;
    }
    session.end.set(end);
    session.frames.set(session.frames.get() + 1);
    ffi::WRITE_STATUS_CONTINUE
}

unsafe extern "C" fn error<H: Handler>(
    _decoder: *const ffi::StreamDecoder,
    status: c_int,
    client_data: *mut c_void,
) {
    // SAFETY: libFLAC passes back the client data given to `init_stream`.
    let session = unsafe { session::<H>(client_data) };
    session.fail(Error::Stream {
        status: ErrorStatus::from_raw(status),
        frame: session.frames.get(),
        offset: session.end.get(),
    });
}

fn state_name(state: c_int) -> &'static str {
    match state {
        0 => "SEARCH_FOR_METADATA",
        1 => "READ_METADATA",
        2 => "SEARCH_FOR_FRAME_SYNC",
        3 => "READ_FRAME",
        4 => "END_OF_STREAM",
        5 => "OGG_ERROR",
        6 => "SEEK_ERROR",
        7 => "ABORTED",
        8 => "MEMORY_ALLOCATION_ERROR",
        9 => "UNINITIALIZED",
        10 => "END_OF_LINK",
        _ => "unknown",
    }
}
