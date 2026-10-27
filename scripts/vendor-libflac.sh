#!/bin/sh
# Re-vendors the libFLAC 1.5.0 stream-decoder sources into crates/libflac from the
# upstream release archive, checking its SHA-256 first. Only the decoder's
# compilation units and the headers they include are kept; the encoder, Ogg,
# metadata editing and file I/O helpers are not.
#
# usage: scripts/vendor-libflac.sh path/to/flac-1.5.0.tar.xz
set -eu

archive=$1
expected=f2c1c76592a82ffff8413ba3c4a1299b6c7ab06c734dee03fd88630485c2b920
actual=$(sha256sum "$archive" | cut -d ' ' -f 1)
if [ "$actual" != "$expected" ]; then
	echo "libFLAC archive SHA-256 is $actual, expected $expected" >&2
	exit 1
fi

root=$(cd "$(dirname "$0")/.." && pwd)
dest=$root/crates/libflac/flac-1.5.0
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
tar -xJf "$archive" -C "$work"

rm -rf "$dest"
mkdir -p "$dest"
cd "$work/flac-1.5.0"
while read -r path; do
	mkdir -p "$dest/$(dirname "$path")"
	cp "$path" "$dest/$path"
done <<EOF
AUTHORS
COPYING.Xiph
include/FLAC/assert.h
include/FLAC/export.h
include/FLAC/format.h
include/FLAC/ordinals.h
include/FLAC/stream_decoder.h
include/share/alloc.h
include/share/compat.h
include/share/endswap.h
src/libFLAC/bitmath.c
src/libFLAC/bitreader.c
src/libFLAC/cpu.c
src/libFLAC/crc.c
src/libFLAC/deduplication/bitreader_read_rice_signed_block.c
src/libFLAC/deduplication/lpc_compute_autocorrelation_intrin.c
src/libFLAC/fixed.c
src/libFLAC/format.c
src/libFLAC/include/private/bitmath.h
src/libFLAC/include/private/bitreader.h
src/libFLAC/include/private/cpu.h
src/libFLAC/include/private/crc.h
src/libFLAC/include/private/fixed.h
src/libFLAC/include/private/float.h
src/libFLAC/include/private/format.h
src/libFLAC/include/private/lpc.h
src/libFLAC/include/private/macros.h
src/libFLAC/include/private/md5.h
src/libFLAC/include/private/memory.h
src/libFLAC/include/protected/stream_decoder.h
src/libFLAC/lpc.c
src/libFLAC/md5.c
src/libFLAC/memory.c
src/libFLAC/stream_decoder.c
EOF
