/* Compile-time checks that the Rust declarations in ffi.rs match libFLAC's
 * layout. ffi.rs declares only the leading fields it reads. */

#include <stddef.h>

#include "FLAC/stream_decoder.h"

#define CHECK(expr) _Static_assert(expr, #expr)

CHECK(sizeof(FLAC__bool) == 4);
CHECK(sizeof(FLAC__StreamDecoderState) == 4);
CHECK(sizeof(FLAC__StreamDecoderErrorStatus) == 4);

CHECK(offsetof(FLAC__StreamMetadata, type) == 0);
CHECK(offsetof(FLAC__StreamMetadata, data.stream_info) == 16);

CHECK(offsetof(FLAC__StreamMetadata_StreamInfo, min_blocksize) == 0);
CHECK(offsetof(FLAC__StreamMetadata_StreamInfo, max_blocksize) == 4);
CHECK(offsetof(FLAC__StreamMetadata_StreamInfo, min_framesize) == 8);
CHECK(offsetof(FLAC__StreamMetadata_StreamInfo, max_framesize) == 12);
CHECK(offsetof(FLAC__StreamMetadata_StreamInfo, sample_rate) == 16);
CHECK(offsetof(FLAC__StreamMetadata_StreamInfo, channels) == 20);
CHECK(offsetof(FLAC__StreamMetadata_StreamInfo, bits_per_sample) == 24);
CHECK(offsetof(FLAC__StreamMetadata_StreamInfo, total_samples) == 32);
CHECK(offsetof(FLAC__StreamMetadata_StreamInfo, md5sum) == 40);
CHECK(sizeof(FLAC__StreamMetadata_StreamInfo) == 56);

CHECK(offsetof(FLAC__Frame, header) == 0);
CHECK(offsetof(FLAC__FrameHeader, blocksize) == 0);
CHECK(offsetof(FLAC__FrameHeader, sample_rate) == 4);
CHECK(offsetof(FLAC__FrameHeader, channels) == 8);
CHECK(offsetof(FLAC__FrameHeader, bits_per_sample) == 16);
