//! Single implementation of the JUCE `juce::var` wire codec.
//!
//! The Rodecaster protocol speaks `juce::ValueTreeSynchroniser` on the wire, so
//! every value (the full-sync initial state, incremental `propertyChanged`
//! updates, and the commands we *send*) is a `juce::var` framed by JUCE's
//! `writeCompressedInt`. This module owns that byte format exactly once, for
//! BOTH directions:
//!
//! - decode: [`crate::valuetree`] (full tree) and the consumer's incremental
//!   parser.
//! - encode: the consumer's command builders, via [`Value::write_to_stream`].
//!
//! Keeping read and write in one place is the whole point: every
//! [`Value::write_to_stream`] output round-trips back through [`read_value`],
//! so the encoder and parser can never silently drift apart.
//!
//! Frame shape (`juce::var::writeToStream`): `writeCompressedInt(1 + payload)`,
//! then a marker byte, then the payload. `writeCompressedInt(n)` is a size byte
//! (low 7 bits = number of little-endian value bytes, capped at 4; high bit =
//! sign) followed by that many value bytes.

/// `juce::var` `VariantStreamMarkers`: the byte after a value's length frame.
pub mod marker {
    /// 32-bit int. JUCE always writes a fixed 4-byte `writeInt`.
    pub const INT: u8 = 0x01;
    pub const BOOL_TRUE: u8 = 0x02;
    pub const BOOL_FALSE: u8 = 0x03;
    /// Double, 8 bytes IEEE 754.
    pub const DOUBLE: u8 = 0x04;
    /// String, UTF-8 with a trailing NUL inside the frame.
    pub const STRING: u8 = 0x05;
    /// 64-bit int, 8 bytes.
    pub const INT64: u8 = 0x06;
    /// Array: a compressed-int element count, then that many values.
    pub const ARRAY: u8 = 0x07;
    /// Binary blob / `MemoryBlock`: the rest of the frame, raw.
    pub const BINARY: u8 = 0x08;
    pub const UNDEFINED: u8 = 0x09;
}

/// A decoded `juce::var`. One type shared by every reader and writer.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Bool(bool),
    /// 32-bit int (marker [`marker::INT`]); JUCE writes a fixed 4 bytes. Stored
    /// widened to `i64` so it can share an accessor with [`Value::Int64`].
    Int(i64),
    /// 64-bit int (marker [`marker::INT64`]).
    Int64(i64),
    /// Double (marker [`marker::DOUBLE`]), 8 bytes IEEE 754.
    Double(f64),
    /// String (marker [`marker::STRING`]). The trailing NUL is stripped on read
    /// and re-added on write.
    String(String),
    /// Array of values (marker [`marker::ARRAY`]).
    Array(Vec<Value>),
    /// Binary blob (marker [`marker::BINARY`]).
    Binary(Vec<u8>),
    /// Void var (marker [`marker::UNDEFINED`], or an empty length frame).
    Undefined,
    /// Unparsed marker; raw payload preserved so the cursor stays aligned.
    Unknown {
        type_id: u8,
        data: Vec<u8>,
    },
}

impl Value {
    /// Extract a bool, if this var is one.
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Value::Bool(b) => Some(*b),
            _ => None,
        }
    }

    /// Extract an integer, widening `Int`/`Int64` to `i64`.
    pub fn as_int(&self) -> Option<i64> {
        match self {
            Value::Int(i) | Value::Int64(i) => Some(*i),
            _ => None,
        }
    }

    /// Render for the diagnostic XML dump.
    pub fn to_xml_string(&self) -> String {
        match self {
            Value::Bool(true) => "1".to_string(),
            Value::Bool(false) => "0".to_string(),
            Value::Int(i) | Value::Int64(i) => i.to_string(),
            Value::Double(d) => {
                // Match the reference XML: 1 decimal place for clean values.
                if d.fract() == 0.0 {
                    format!("{:.1}", d)
                } else {
                    format!("{}", d)
                }
            }
            Value::String(s) => s.clone(),
            Value::Array(arr) => arr
                .iter()
                .map(|v| v.to_xml_string())
                .collect::<Vec<_>>()
                .join(","),
            Value::Binary(data) => data.iter().map(|b| format!("{:02x}", b)).collect(),
            Value::Undefined => String::new(),
            Value::Unknown { type_id, data } => {
                format!("unknown_0x{:02x}_{}_bytes", type_id, data.len())
            }
        }
    }

    /// Encode as `juce::var::writeToStream`: `writeCompressedInt(1 + payload)`,
    /// the marker byte, then the payload.
    pub fn write_to_stream(&self, out: &mut Vec<u8>) {
        match self {
            Value::Bool(b) => {
                write_compressed_int(out, 1);
                out.push(if *b {
                    marker::BOOL_TRUE
                } else {
                    marker::BOOL_FALSE
                });
            }
            Value::Int(i) => {
                write_compressed_int(out, 5);
                out.push(marker::INT);
                out.extend_from_slice(&(*i as i32).to_le_bytes());
            }
            Value::Int64(i) => {
                write_compressed_int(out, 9);
                out.push(marker::INT64);
                out.extend_from_slice(&i.to_le_bytes());
            }
            Value::Double(d) => {
                write_compressed_int(out, 9);
                out.push(marker::DOUBLE);
                out.extend_from_slice(&d.to_bits().to_le_bytes());
            }
            Value::String(s) => {
                let bytes = s.as_bytes();
                // Payload is UTF-8 plus a trailing NUL; the frame counts both.
                write_compressed_int(out, 1 + bytes.len() as i64 + 1);
                out.push(marker::STRING);
                out.extend_from_slice(bytes);
                out.push(0);
            }
            Value::Binary(data) => {
                write_compressed_int(out, 1 + data.len() as i64);
                out.push(marker::BINARY);
                out.extend_from_slice(data);
            }
            Value::Array(values) => {
                // Serialize the body first so the length frame can count it.
                let mut body = Vec::new();
                write_compressed_int(&mut body, values.len() as i64);
                for v in values {
                    v.write_to_stream(&mut body);
                }
                write_compressed_int(out, 1 + body.len() as i64);
                out.push(marker::ARRAY);
                out.extend_from_slice(&body);
            }
            Value::Undefined => {
                write_compressed_int(out, 1);
                out.push(marker::UNDEFINED);
            }
            Value::Unknown { type_id, data } => {
                write_compressed_int(out, 1 + data.len() as i64);
                out.push(*type_id);
                out.extend_from_slice(data);
            }
        }
    }
}

// Free-function codec over `(data, &mut pos)` so any cursor can delegate.

fn take_u8(data: &[u8], pos: &mut usize) -> Option<u8> {
    let b = *data.get(*pos)?;
    *pos += 1;
    Some(b)
}

fn take_bytes<'a>(data: &'a [u8], pos: &mut usize, n: usize) -> Option<&'a [u8]> {
    let end = pos.checked_add(n)?;
    if end <= data.len() {
        let slice = &data[*pos..end];
        *pos = end;
        Some(slice)
    } else {
        None
    }
}

/// Read a JUCE `writeCompressedInt`: one size byte (low 7 bits = value byte
/// count, capped at 4; high bit = sign) then that many little-endian bytes.
pub fn read_compressed_int(data: &[u8], pos: &mut usize) -> Option<i64> {
    let size_byte = take_u8(data, pos)?;
    let num_bytes = (size_byte & 0x7f) as usize;
    if num_bytes == 0 {
        return Some(0);
    }
    if num_bytes > 4 {
        // JUCE treats this as corrupt data and bails.
        return None;
    }
    let bytes = take_bytes(data, pos, num_bytes)?;
    let mut value: i64 = 0;
    for (i, &b) in bytes.iter().enumerate() {
        value |= (b as i64) << (i * 8);
    }
    if size_byte & 0x80 != 0 {
        value = -value;
    }
    Some(value)
}

/// Write a JUCE `writeCompressedInt` (the inverse of [`read_compressed_int`]).
pub fn write_compressed_int(out: &mut Vec<u8>, value: i64) {
    let negative = value < 0;
    let mut magnitude = value.unsigned_abs();
    let mut bytes = [0u8; 4];
    let mut num_bytes = 0usize;
    while magnitude != 0 && num_bytes < 4 {
        bytes[num_bytes] = (magnitude & 0xff) as u8;
        magnitude >>= 8;
        num_bytes += 1;
    }
    let size_byte = num_bytes as u8 | if negative { 0x80 } else { 0 };
    out.push(size_byte);
    out.extend_from_slice(&bytes[..num_bytes]);
}

/// Read a NUL-terminated UTF-8 string (`juce::OutputStream::writeString`).
pub fn read_cstring<'a>(data: &'a [u8], pos: &mut usize) -> Option<&'a str> {
    let start = *pos;
    let rel = data.get(start..)?.iter().position(|&b| b == 0)?;
    let s = std::str::from_utf8(&data[start..start + rel]).ok()?;
    *pos = start + rel + 1; // skip the NUL
    Some(s)
}

/// Read a single `juce::var` (`var::readFromStream`).
///
/// A `writeCompressedInt` length frame (`1 + payload_len`), then the marker
/// byte, then the payload. A zero-length frame is a void var.
pub fn read_value(data: &[u8], pos: &mut usize) -> Option<Value> {
    let data_len = read_compressed_int(data, pos)?;
    if data_len <= 0 {
        // numBytes == 0 -> JUCE returns a void var.
        return Some(Value::Undefined);
    }

    let type_byte = take_u8(data, pos)?;
    let value_len = (data_len - 1) as usize;

    match type_byte {
        marker::INT => {
            // JUCE writes a fixed 4-byte `writeInt`.
            let b = take_bytes(data, pos, 4)?;
            Some(Value::Int(
                i32::from_le_bytes([b[0], b[1], b[2], b[3]]) as i64
            ))
        }
        marker::BOOL_TRUE => Some(Value::Bool(true)),
        marker::BOOL_FALSE => Some(Value::Bool(false)),
        marker::DOUBLE => {
            let b = take_bytes(data, pos, 8)?;
            Some(Value::Double(f64::from_le_bytes([
                b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7],
            ])))
        }
        marker::STRING => {
            let b = take_bytes(data, pos, value_len)?;
            let s = std::str::from_utf8(b)
                .ok()?
                .trim_end_matches('\0')
                .to_string();
            Some(Value::String(s))
        }
        marker::INT64 => {
            let b = take_bytes(data, pos, 8)?;
            Some(Value::Int64(i64::from_le_bytes([
                b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7],
            ])))
        }
        marker::ARRAY => {
            let count = usize::try_from(read_compressed_int(data, pos)?).ok()?;
            // Cap the pre-allocation: a value is at least 1 byte on the wire.
            let remaining = data.len().saturating_sub(*pos);
            let mut values = Vec::with_capacity(count.min(remaining));
            for _ in 0..count {
                values.push(read_value(data, pos)?);
            }
            Some(Value::Array(values))
        }
        marker::BINARY => {
            let b = take_bytes(data, pos, value_len)?;
            Some(Value::Binary(b.to_vec()))
        }
        marker::UNDEFINED => Some(Value::Undefined),
        _ => {
            let b = take_bytes(data, pos, value_len)?;
            Some(Value::Unknown {
                type_id: type_byte,
                data: b.to_vec(),
            })
        }
    }
}

/// A position-tracking reader over a byte slice: the shared cursor for the
/// `juce::var` decode (the full ValueTree walk in [`crate::valuetree`]).
pub struct Reader<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0 }
    }

    /// Bytes left from the current position.
    pub fn remaining(&self) -> usize {
        self.data.len().saturating_sub(self.pos)
    }

    pub fn read_u8(&mut self) -> Option<u8> {
        take_u8(self.data, &mut self.pos)
    }

    pub fn read_compressed_int(&mut self) -> Option<i64> {
        read_compressed_int(self.data, &mut self.pos)
    }

    pub fn read_cstring(&mut self) -> Option<&'a str> {
        read_cstring(self.data, &mut self.pos)
    }

    pub fn read_value(&mut self) -> Option<Value> {
        read_value(self.data, &mut self.pos)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Encode a value, decode it back, and assert it survives unchanged and the
    /// frame is fully consumed.
    fn assert_round_trip(value: Value) {
        let mut buf = Vec::new();
        value.write_to_stream(&mut buf);
        let mut pos = 0;
        let decoded = read_value(&buf, &mut pos).expect("decodes");
        assert_eq!(decoded, value, "round-trip value mismatch");
        assert_eq!(pos, buf.len(), "frame not fully consumed for {value:?}");
    }

    #[test]
    fn round_trips_every_variant() {
        assert_round_trip(Value::Bool(true));
        assert_round_trip(Value::Bool(false));
        assert_round_trip(Value::Int(75));
        assert_round_trip(Value::Int(-1));
        assert_round_trip(Value::Int(i32::MIN as i64));
        assert_round_trip(Value::Int64(1 << 40));
        assert_round_trip(Value::Double(0.5));
        assert_round_trip(Value::Double(1.0));
        assert_round_trip(Value::String("0.472441|0.472441".to_string()));
        assert_round_trip(Value::String(String::new()));
        assert_round_trip(Value::Binary(vec![0x01, 0x01, 0x02, 0x01, 0x01, 0x02]));
        assert_round_trip(Value::Array(vec![Value::Int(1), Value::Int(2)]));
        assert_round_trip(Value::Undefined);
    }

    #[test]
    fn compressed_int_round_trips() {
        for v in [0i64, 1, 5, 7, 9, 300, -5, 0xFFFF, 0x0100_0000] {
            let mut buf = Vec::new();
            write_compressed_int(&mut buf, v);
            let mut pos = 0;
            assert_eq!(read_compressed_int(&buf, &mut pos), Some(v), "for {v}");
            assert_eq!(pos, buf.len());
        }
    }

    #[test]
    fn write_matches_known_juce_frames() {
        // These are the exact byte vectors the hand-rolled encoders emitted
        // before this module existed: lock them so the codec stays compatible
        // with the device's parser.
        let mut buf = Vec::new();
        Value::Bool(true).write_to_stream(&mut buf);
        assert_eq!(buf, [0x01, 0x01, 0x02]); // var bool true

        buf.clear();
        Value::Bool(false).write_to_stream(&mut buf);
        assert_eq!(buf, [0x01, 0x01, 0x03]); // var bool false

        buf.clear();
        Value::Int(75).write_to_stream(&mut buf);
        assert_eq!(buf, [0x01, 0x05, 0x01, 0x4b, 0x00, 0x00, 0x00]); // var int

        buf.clear();
        Value::Int(0xFFFF_FFFFu32 as i64).write_to_stream(&mut buf);
        assert_eq!(buf, [0x01, 0x05, 0x01, 0xff, 0xff, 0xff, 0xff]); // source_id = -1

        buf.clear();
        Value::Binary(vec![0x01, 0x01, 0x02, 0x01, 0x01, 0x02]).write_to_stream(&mut buf);
        assert_eq!(
            buf,
            [0x01, 0x07, 0x08, 0x01, 0x01, 0x02, 0x01, 0x01, 0x02] // mix link/unlink request
        );
    }

    #[test]
    fn reads_empty_frame_as_void() {
        // A zero-length frame (compressedInt(0)) decodes to a void var.
        let mut pos = 0;
        assert_eq!(read_value(&[0x00], &mut pos), Some(Value::Undefined));
        assert_eq!(pos, 1);
    }

    #[test]
    fn rejects_oversized_compressed_int() {
        // More than 4 value bytes is corrupt per JUCE.
        let mut pos = 0;
        assert_eq!(read_compressed_int(&[0x05, 1, 2, 3, 4, 5], &mut pos), None);
    }
}
