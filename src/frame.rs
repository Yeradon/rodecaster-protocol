//! TCP stream packet framing codec.
//!
//! Frames protocol messages using a 4-byte magic header (`0xF2B49E2C`)
//! and a 4-byte little-endian length prefix:
//! `[magic (4B)][length (4B)][payload]`.

/// Magic header for RODECaster protocol packets (little-endian on the wire).
pub const MAGIC_HEADER: u32 = 0xF2B49E2C;

/// A length-prefixed protocol packet.
#[derive(Debug, Clone)]
pub struct Packet {
    pub header: u32,
    pub length: u32,
    pub payload: Vec<u8>,
}

impl Packet {
    pub fn new(payload: Vec<u8>) -> Self {
        Packet {
            header: MAGIC_HEADER,
            length: payload.len() as u32,
            payload,
        }
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(8 + self.payload.len());
        out.extend_from_slice(&self.header.to_le_bytes());
        out.extend_from_slice(&self.length.to_le_bytes());
        out.extend_from_slice(&self.payload);
        out
    }

    /// Parse a packet from bytes, returning it and the total bytes consumed.
    pub fn from_bytes(data: &[u8]) -> Option<(Self, usize)> {
        if data.len() < 8 {
            return None;
        }

        let header = u32::from_le_bytes([data[0], data[1], data[2], data[3]]);
        if header != MAGIC_HEADER {
            return None;
        }

        let length = u32::from_le_bytes([data[4], data[5], data[6], data[7]]) as usize;
        let total_len = 8 + length;

        if data.len() < total_len {
            return None;
        }

        Some((
            Packet {
                header,
                length: length as u32,
                payload: data[8..total_len].to_vec(),
            },
            total_len,
        ))
    }
}

/// Result of scanning a byte buffer for a leading frame without allocating.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrameScan {
    /// Exactly one complete frame of `len` bytes starts at offset 0.
    Complete { len: usize },
    /// Valid magic header, but the full frame has not arrived yet.
    Incomplete,
    /// Offset 0 does not match the magic header.
    Desync,
}

/// Scan `buf` for a complete frame at offset 0 without allocating.
pub fn scan_frame(buf: &[u8]) -> FrameScan {
    if buf.len() < 8 {
        return FrameScan::Incomplete;
    }
    let header = u32::from_le_bytes([buf[0], buf[1], buf[2], buf[3]]);
    if header != MAGIC_HEADER {
        return FrameScan::Desync;
    }
    let length = u32::from_le_bytes([buf[4], buf[5], buf[6], buf[7]]) as usize;
    let total = 8 + length;
    if buf.len() < total {
        return FrameScan::Incomplete;
    }
    FrameScan::Complete { len: total }
}

/// Borrow the payload of one complete frame at offset 0 without copying.
pub fn frame_payload(buf: &[u8]) -> Option<&[u8]> {
    match scan_frame(buf) {
        FrameScan::Complete { len } => Some(&buf[8..len]),
        FrameScan::Incomplete | FrameScan::Desync => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_packet_roundtrip() {
        let payload = vec![0x01, 0x02, 0x03, 0x04];
        let packet = Packet::new(payload.clone());
        let bytes = packet.to_bytes();

        let (parsed, len) = Packet::from_bytes(&bytes).unwrap();
        assert_eq!(len, bytes.len());
        assert_eq!(parsed.payload, payload);
    }

    #[test]
    fn to_bytes_layout_is_magic_len_payload() {
        let packet = Packet::new(vec![0xAA, 0xBB]);
        let bytes = packet.to_bytes();
        assert_eq!(&bytes[0..4], &0xF2B4_9E2Cu32.to_le_bytes()); // magic, LE
        assert_eq!(&bytes[4..8], &2u32.to_le_bytes()); // length, LE
        assert_eq!(&bytes[8..], &[0xAA, 0xBB]);
    }

    #[test]
    fn scan_frame_complete_returns_total_len() {
        let bytes = Packet::new(vec![0xAA, 0xBB, 0xCC]).to_bytes();
        assert_eq!(scan_frame(&bytes), FrameScan::Complete { len: bytes.len() });
    }

    #[test]
    fn scan_frame_incomplete_when_shorter_than_header_or_frame() {
        assert_eq!(scan_frame(&[]), FrameScan::Incomplete);
        assert_eq!(scan_frame(&[0x2c, 0x9e, 0xb4]), FrameScan::Incomplete); // <8 bytes
        let full = Packet::new(vec![1, 2, 3, 4, 5]).to_bytes();
        // Valid magic + length, but one payload byte short.
        assert_eq!(scan_frame(&full[..full.len() - 1]), FrameScan::Incomplete);
    }

    #[test]
    fn scan_frame_desync_on_bad_magic() {
        assert_eq!(scan_frame(&[0, 0, 0, 0, 0, 0, 0, 0]), FrameScan::Desync);
    }

    #[test]
    fn scan_frame_stops_at_first_frame_boundary() {
        let first = Packet::new(vec![0x09]).to_bytes();
        let first_len = first.len();
        let mut two = first;
        two.extend_from_slice(&Packet::new(vec![0x08]).to_bytes());
        assert_eq!(scan_frame(&two), FrameScan::Complete { len: first_len });
    }

    #[test]
    fn frame_payload_borrows_inner_bytes() {
        let payload = vec![0x02, 0xDE, 0xAD];
        let bytes = Packet::new(payload.clone()).to_bytes();
        assert_eq!(frame_payload(&bytes), Some(&payload[..]));
        // First frame's payload only, ignoring a trailing second frame.
        let mut two = bytes;
        two.extend_from_slice(&Packet::new(vec![0xFF]).to_bytes());
        assert_eq!(frame_payload(&two), Some(&payload[..]));
        // Incomplete / desync borrow nothing.
        assert_eq!(frame_payload(&[0x2c, 0x9e]), None);
        assert_eq!(frame_payload(&[0, 0, 0, 0, 0, 0, 0, 0]), None);
    }
}
