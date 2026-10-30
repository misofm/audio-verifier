#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")"
FFMPEG="${FFMPEG:-ffmpeg}"
FLAC="${FLAC:-flac}"
STEREO="0.5*sin(2*PI*440*t)+0.05*sin(2*PI*5000*t)|0.4*sin(2*PI*660*t)+0.05*sin(2*PI*7000*t)"
MONO="0.5*sin(2*PI*440*t)+0.05*sin(2*PI*5000*t)"

canonicalize() {
  python3 -I -c '
import sys
data = open(sys.argv[1], "rb").read()
assert data[:4] == b"fLaC"
offset = 4
streaminfo = None
while True:
    header = data[offset]
    length = int.from_bytes(data[offset + 1:offset + 4], "big")
    if header & 0x7f == 0:
        streaminfo = data[offset + 4:offset + 4 + length]
    offset += 4 + length
    if header & 0x80:
        break
assert streaminfo is not None and len(streaminfo) == 34
out = b"fLaC" + bytes([0x80]) + (34).to_bytes(3, "big") + streaminfo + data[offset:]
open(sys.argv[2], "wb").write(out)
' "$1" "$2"
}

encode() {
  local name="$1" expr="$2" rate="$3" duration="$4"
  shift 4
  "$FFMPEG" -hide_banner -loglevel error -y -bitexact -f lavfi \
    -i "aevalsrc=${expr}:s=${rate}:d=${duration}" \
    -map_metadata -1 -fflags +bitexact -flags:a +bitexact -c:a flac "$@" "raw-${name}.flac"
  canonicalize "raw-${name}.flac" "${name}.flac"
  rm "raw-${name}.flac"
}

encode_libflac() {
  local name="$1" expr="$2" rate="$3" duration="$4" codec="$5"
  "$FFMPEG" -hide_banner -loglevel error -y -bitexact -f lavfi \
    -i "aevalsrc=${expr}:s=${rate}:d=${duration}" \
    -map_metadata -1 -fflags +bitexact -flags:a +bitexact -c:a "$codec" "raw-${name}.wav"
  "$FLAC" --silent --force --no-padding --no-seektable -o "raw-${name}.flac" "raw-${name}.wav"
  canonicalize "raw-${name}.flac" "${name}.flac"
  rm "raw-${name}.wav" "raw-${name}.flac"
}

encode s16_stereo_44100 "$STEREO" 44100 0.25 -sample_fmt s16
encode s24_stereo_48000 "$STEREO" 48000 0.25 -sample_fmt s32 -bits_per_raw_sample 24
encode s16_mono_96000 "$MONO" 96000 0.25 -sample_fmt s16
encode s16_mono_44100 "$MONO" 44100 0.2 -sample_fmt s16
encode s16_stereo_22050 "$STEREO" 22050 0.1 -sample_fmt s16
encode s32_stereo_48000 "$STEREO" 48000 0.05 -sample_fmt s32 -bits_per_raw_sample 32 -strict experimental
encode s16_stereo_44100_short "$STEREO" 44100 0.05 -sample_fmt s16

encode_libflac libflac_s16_stereo_44100 "$STEREO" 44100 0.25 pcm_s16le
encode_libflac libflac_s24_stereo_96000 "$STEREO" 96000 0.1 pcm_s24le
encode_libflac libflac_s16_mono_48000_short "$MONO" 48000 0.0002 pcm_s16le
