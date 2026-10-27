//! Parses the frame header at the start of each frame libFLAC decoded.
//!
//! libFLAC reports neither a frame's blocking strategy nor its coded number as
//! written, and accepts overlong number encodings. Reading the header from the
//! frame's own bytes recovers both, and checking its CRC-8 and block size against
//! libFLAC's confirms the frame starts exactly where the previous one ended.

/// The header fields libFLAC does not report.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct FrameHeader {
    /// Variable blocking: the coded number is a sample number, not a frame number.
    pub variable: bool,
    /// The coded frame or sample number.
    pub number: u64,
    /// Samples per channel.
    pub block_size: u32,
}

/// Parses the header at the start of `bytes`, a whole frame.
pub(crate) fn parse(bytes: &[u8]) -> Result<FrameHeader, &'static str> {
    let mut reader = Reader { bytes, at: 0 };
    let [sync, flags, sizes, _layout] = reader.array()?;
    if sync != 0xff || flags & 0xfe != 0xf8 {
        return Err("no frame sync code");
    }
    let variable = flags & 1 == 1;
    let number = reader.coded_number(if variable { 7 } else { 6 })?;
    let block_size = match sizes >> 4 {
        0 => return Err("reserved block size code"),
        1 => 192,
        code @ 2..=5 => 576 << (code - 2),
        6 => u32::from(reader.byte()?) + 1,
        7 => u32::from(u16::from_be_bytes(reader.array()?)) + 1,
        code => 256 << (code - 8),
    };
    match sizes & 0x0f {
        12 => {
            reader.byte()?;
        }
        13 | 14 => {
            reader.array::<2>()?;
        }
        15 => return Err("forbidden sample rate code"),
        _ => {}
    }
    let end = reader.at;
    if reader.byte()? != crc8(&bytes[..end]) {
        return Err("header CRC-8 mismatch");
    }
    Ok(FrameHeader {
        variable,
        number,
        block_size,
    })
}

struct Reader<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl Reader<'_> {
    fn byte(&mut self) -> Result<u8, &'static str> {
        let byte = *self.bytes.get(self.at).ok_or("truncated frame header")?;
        self.at += 1;
        Ok(byte)
    }

    fn array<const N: usize>(&mut self) -> Result<[u8; N], &'static str> {
        let mut array = [0; N];
        for byte in &mut array {
            *byte = self.byte()?;
        }
        Ok(array)
    }

    /// Reads a frame or sample number in FLAC's extended UTF-8 coding, at most
    /// `max_len` bytes long and in its shortest form (RFC 9639 9.1.5).
    fn coded_number(&mut self, max_len: u32) -> Result<u64, &'static str> {
        let first = self.byte()?;
        let len = first.leading_ones();
        if len == 0 {
            return Ok(u64::from(first));
        }
        if len == 1 || len > max_len {
            return Err("invalid coded number");
        }
        let mut value = u64::from(first & (0x7f >> len));
        for _ in 1..len {
            let byte = self.byte()?;
            if byte & 0xc0 != 0x80 {
                return Err("invalid coded number");
            }
            value = value << 6 | u64::from(byte & 0x3f);
        }
        // A `len`-byte coding holds 5 * len + 1 bits and a one-byte coding 7, so
        // the shortest form uses `len` bytes only from the next power up.
        let shorter_bits = if len == 2 { 7 } else { 5 * (len - 1) + 1 };
        if value < 1 << shorter_bits {
            return Err("overlong coded number");
        }
        Ok(value)
    }
}

/// CRC-8 with polynomial x^8 + x^2 + x + 1, as in the FLAC frame header.
fn crc8(bytes: &[u8]) -> u8 {
    bytes.iter().fold(0, |crc, &byte| {
        (0..8).fold(crc ^ byte, |crc, _| {
            if crc & 0x80 == 0 {
                crc << 1
            } else {
                crc << 1 ^ 0x07
            }
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Encodes `value` in exactly `len` bytes of FLAC's extended UTF-8 coding.
    fn coded(value: u64, len: u32) -> Vec<u8> {
        if len == 1 {
            return vec![value as u8];
        }
        let mut bytes = vec![0; len as usize];
        for (i, byte) in bytes.iter_mut().enumerate().skip(1).rev() {
            *byte = 0x80 | ((value >> (6 * (len as usize - 1 - i))) & 0x3f) as u8;
        }
        let lead = !(0xffu8 >> len);
        bytes[0] = lead | (value >> (6 * (len - 1))) as u8;
        bytes
    }

    /// A 4096-sample frame header carrying `number`, CRC-8 included.
    fn header(variable: bool, number: &[u8]) -> Vec<u8> {
        let mut bytes = vec![0xff, if variable { 0xf9 } else { 0xf8 }, 0xc9, 0x18];
        bytes.extend_from_slice(number);
        bytes.push(crc8(&bytes));
        bytes
    }

    #[test]
    fn coded_numbers_accept_the_shortest_form_at_every_length() {
        let shortest: [(u64, u32); 7] = [
            (0x7f, 1),
            (0x80, 2),
            (0x800, 3),
            (0x1_0000, 4),
            (0x20_0000, 5),
            (0x400_0000, 6),
            (0x8000_0000, 7),
        ];
        for (value, len) in shortest {
            let parsed = parse(&header(true, &coded(value, len))).expect("shortest form");
            assert_eq!(parsed.number, value, "{len} bytes");
            if len > 1 {
                assert_eq!(
                    parse(&header(true, &coded(value - 1, len))),
                    Err("overlong coded number"),
                    "{len} bytes"
                );
            }
        }
    }

    #[test]
    fn frame_numbers_are_at_most_six_bytes() {
        let seven = coded(0x8000_0000, 7);
        assert_eq!(parse(&header(false, &seven)), Err("invalid coded number"));
        assert_eq!(
            parse(&header(true, &seven)).map(|h| h.number),
            Ok(0x8000_0000)
        );
    }

    #[test]
    fn header_fields_and_crc_are_checked() {
        let good = header(false, &[5]);
        assert_eq!(
            parse(&good),
            Ok(FrameHeader {
                variable: false,
                number: 5,
                block_size: 4096
            })
        );
        let mut bad_crc = good.clone();
        *bad_crc.last_mut().unwrap() ^= 1;
        assert_eq!(parse(&bad_crc), Err("header CRC-8 mismatch"));
        assert_eq!(
            parse(&good[..good.len() - 1]),
            Err("truncated frame header")
        );
        assert_eq!(
            parse(&[0xff, 0xfa, 0xc9, 0x18, 0]),
            Err("no frame sync code")
        );
        assert_eq!(parse(&header(false, &[0x80])), Err("invalid coded number"));
    }
}
