//! The part of libFLAC 1.5.0's `FLAC/stream_decoder.h` this crate calls.
//! `src/layout.c` checks the struct offsets at build time.

use std::ffi::{c_int, c_void};

pub type Bool = c_int;

#[repr(C)]
pub struct StreamDecoder {
    _private: [u8; 0],
}

pub const STATE_END_OF_STREAM: c_int = 4;
pub const STATE_ABORTED: c_int = 7;
pub const STATE_MEMORY_ALLOCATION_ERROR: c_int = 8;

pub const INIT_STATUS_OK: c_int = 0;

pub const READ_STATUS_CONTINUE: c_int = 0;
pub const READ_STATUS_END_OF_STREAM: c_int = 1;
pub const READ_STATUS_ABORT: c_int = 2;

pub const TELL_STATUS_OK: c_int = 0;

pub const WRITE_STATUS_CONTINUE: c_int = 0;
pub const WRITE_STATUS_ABORT: c_int = 1;

pub const METADATA_TYPE_STREAMINFO: c_int = 0;

#[repr(C)]
pub struct StreamInfo {
    pub min_blocksize: u32,
    pub max_blocksize: u32,
    pub min_framesize: u32,
    pub max_framesize: u32,
    pub sample_rate: u32,
    pub channels: u32,
    pub bits_per_sample: u32,
    pub total_samples: u64,
    pub md5sum: [u8; 16],
}

/// Leading fields of `FLAC__StreamMetadata`; `data` is a union whose
/// `stream_info` member is read only when `type_` is STREAMINFO.
#[repr(C)]
pub struct StreamMetadata {
    pub type_: c_int,
    pub is_last: Bool,
    pub length: u32,
    pub stream_info: StreamInfo,
}

/// Leading fields of `FLAC__FrameHeader`, the first member of `FLAC__Frame`.
#[repr(C)]
pub struct FrameHeader {
    pub blocksize: u32,
    pub sample_rate: u32,
    pub channels: u32,
    pub channel_assignment: c_int,
    pub bits_per_sample: u32,
}

pub type ReadCallback =
    unsafe extern "C" fn(*const StreamDecoder, *mut u8, *mut usize, *mut c_void) -> c_int;
pub type SeekCallback = unsafe extern "C" fn(*const StreamDecoder, u64, *mut c_void) -> c_int;
pub type TellCallback = unsafe extern "C" fn(*const StreamDecoder, *mut u64, *mut c_void) -> c_int;
pub type LengthCallback =
    unsafe extern "C" fn(*const StreamDecoder, *mut u64, *mut c_void) -> c_int;
pub type EofCallback = unsafe extern "C" fn(*const StreamDecoder, *mut c_void) -> Bool;
pub type WriteCallback = unsafe extern "C" fn(
    *const StreamDecoder,
    *const FrameHeader,
    *const *const i32,
    *mut c_void,
) -> c_int;
pub type MetadataCallback =
    unsafe extern "C" fn(*const StreamDecoder, *const StreamMetadata, *mut c_void);
pub type ErrorCallback = unsafe extern "C" fn(*const StreamDecoder, c_int, *mut c_void);

unsafe extern "C" {
    pub fn FLAC__stream_decoder_new() -> *mut StreamDecoder;
    pub fn FLAC__stream_decoder_delete(decoder: *mut StreamDecoder);
    #[allow(clippy::too_many_arguments)]
    pub fn FLAC__stream_decoder_init_stream(
        decoder: *mut StreamDecoder,
        read: Option<ReadCallback>,
        seek: Option<SeekCallback>,
        tell: Option<TellCallback>,
        length: Option<LengthCallback>,
        eof: Option<EofCallback>,
        write: Option<WriteCallback>,
        metadata: Option<MetadataCallback>,
        error: Option<ErrorCallback>,
        client_data: *mut c_void,
    ) -> c_int;
    pub fn FLAC__stream_decoder_process_until_end_of_metadata(decoder: *mut StreamDecoder) -> Bool;
    pub fn FLAC__stream_decoder_process_until_end_of_stream(decoder: *mut StreamDecoder) -> Bool;
    pub fn FLAC__stream_decoder_get_state(decoder: *const StreamDecoder) -> c_int;
    pub fn FLAC__stream_decoder_get_decode_position(
        decoder: *const StreamDecoder,
        position: *mut u64,
    ) -> Bool;
}
