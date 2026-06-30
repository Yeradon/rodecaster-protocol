//! USB HID transport framing and report chunking.
//!
//! When communicating with a RØDECaster over USB, messages are split into
//! fixed [`REPORT_SIZE`] (64-byte) HID reports. Each report begins with a
//! report ID byte ([`REPORT_ID_OUT`] host-to-device, [`REPORT_ID_IN`] device-to-host)
//! followed by up to [`REPORT_PAYLOAD`] (63 bytes) of data.
//!
//! This module provides packet encoding and scanning so you can assemble
//! incoming HID reports into complete protocol payloads and split outgoing
//! messages into valid HID reports.
//!
//! # Sending Messages
//!
//! To send a message, wrap your payload in [`Packet`] and call [`Packet::to_bytes`]:
//!
//! ```rust
//! use rodecaster_protocol::usb::{Packet, REPORT_SIZE};
//!
//! let payload = vec![0x01, 0x02, 0x03];
//! let report_bytes = Packet::new(payload).to_bytes();
//!
//! // report_bytes is padded to multiples of REPORT_SIZE (64 bytes).
//! // Write each 64-byte report to your HID device handle:
//! for report in report_bytes.chunks_exact(REPORT_SIZE) {
//!     let _ = report;
//! }
//! ```
//!
//! # Receiving Messages
//!
//! As reports arrive from your HID device, append them to a buffer and call
//! [`scan_frame`]:
//!
//! ```rust
//! use rodecaster_protocol::usb::{scan_frame, FrameScan, Packet};
//!
//! let mut buffer = Vec::new();
//! // buffer.extend_from_slice(&incoming_report);
//!
//! if let FrameScan::Complete { len } = scan_frame(&buffer) {
//!     if let Some((packet, consumed)) = Packet::from_bytes(&buffer[..len]) {
//!         buffer.drain(..consumed);
//!         // Pass packet.payload to ProtocolSession::ingest
//!         let _ = packet.payload;
//!     }
//! }
//! ```

pub use crate::frame::FrameScan;

/// HID report id for host -> device (output) reports.
pub const REPORT_ID_OUT: u8 = 0x03;

/// HID report id for device -> host (input) reports.
pub const REPORT_ID_IN: u8 = 0x04;

/// Total HID report size on the wire: one report-id byte plus the payload.
pub const REPORT_SIZE: usize = 256;

/// Usable payload bytes per HID report ([`REPORT_SIZE`] minus the report-id
/// byte). The length-prefixed message is chunked into pieces this large.
pub const REPORT_PAYLOAD: usize = REPORT_SIZE - 1;

/// Sanity cap on a decoded message body (1 MiB). A corrupt length prefix would
/// otherwise wedge a streaming reader on an impossible frame; past this,
/// [`scan_frame`] reports [`FrameScan::Desync`].
pub const MAX_MESSAGE_LEN: usize = 1024 * 1024;

/// The 4-byte body the device expects as the first message after connect to
/// start a full-state sync. Send [`handshake_bytes`]; on the wire it is the
/// framed form `[04 00 00 00][AD 10 A7 B0]` in one report.
pub const HANDSHAKE_BODY: [u8; 4] = [0xAD, 0x10, 0xA7, 0xB0];

/// A USB HID message: the inner `body` plus its length, framed and chunked into
/// HID reports on the wire. The USB counterpart to [`crate::Packet`] (no magic
/// header).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Packet {
    pub length: u32,
    pub payload: Vec<u8>,
}

impl Packet {
    #[must_use]
    pub fn new(payload: Vec<u8>) -> Self {
        Packet {
            length: payload.len() as u32,
            payload,
        }
    }

    /// Serialize to the full HID wire form: `[u32 LE len][body]` chunked into
    /// [`REPORT_PAYLOAD`]-byte pieces, each prefixed with [`REPORT_ID_OUT`] and
    /// the last zero-padded to [`REPORT_SIZE`]. Write it to the device in
    /// [`REPORT_SIZE`]-byte chunks, one report per HID write.
    #[must_use]
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut framed = Vec::with_capacity(4 + self.payload.len());
        framed.extend_from_slice(&self.length.to_le_bytes());
        framed.extend_from_slice(&self.payload);

        let n_reports = framed.len().div_ceil(REPORT_PAYLOAD).max(1);
        let mut out = vec![0u8; n_reports * REPORT_SIZE];
        for (i, chunk) in framed.chunks(REPORT_PAYLOAD).enumerate() {
            let base = i * REPORT_SIZE;
            out[base] = REPORT_ID_OUT;
            out[base + 1..=base + chunk.len()].copy_from_slice(chunk);
        }
        out
    }

    /// Parse one message from a buffer of raw HID reports (each [`REPORT_SIZE`]
    /// bytes as read from the device), returning it and the raw byte count to
    /// drain (a whole number of reports). `None` unless `buf` begins with a
    /// complete, valid message. The report-id byte of each report is ignored, so
    /// this decodes both [`REPORT_ID_OUT`] and [`REPORT_ID_IN`] streams.
    #[must_use]
    pub fn from_bytes(buf: &[u8]) -> Option<(Self, usize)> {
        let (body_len, n_reports) = match scan_frame(buf) {
            FrameScan::Complete { len } => (frame_len_unchecked(buf), len / REPORT_SIZE),
            FrameScan::Incomplete | FrameScan::Desync => return None,
        };
        // De-chunk: concatenate each report's payload (skipping the report-id
        // byte), then take the body that follows the 4-byte length prefix.
        let mut framed = Vec::with_capacity(n_reports * REPORT_PAYLOAD);
        for i in 0..n_reports {
            let base = i * REPORT_SIZE;
            framed.extend_from_slice(&buf[base + 1..base + REPORT_SIZE]);
        }
        let body = framed[4..4 + body_len].to_vec();
        Some((
            Packet {
                length: body_len as u32,
                payload: body,
            },
            n_reports * REPORT_SIZE,
        ))
    }
}

/// Read the 4-byte length prefix from the first report's payload. Caller must
/// have confirmed `buf.len() >= REPORT_SIZE` (e.g. via [`scan_frame`]).
fn frame_len_unchecked(buf: &[u8]) -> usize {
    u32::from_le_bytes([buf[1], buf[2], buf[3], buf[4]]) as usize
}

/// Scan a raw HID report buffer for a complete leading message without copying
/// the body. The USB counterpart to [`crate::scan_frame`]; `len` in
/// [`FrameScan::Complete`] is the raw byte count to drain (a whole number of
/// [`REPORT_SIZE`] reports). [`FrameScan::Desync`] means the length prefix is
/// impossible (zero or over [`MAX_MESSAGE_LEN`]); drop one report and rescan.
#[must_use]
pub fn scan_frame(buf: &[u8]) -> FrameScan {
    if buf.len() < REPORT_SIZE {
        return FrameScan::Incomplete;
    }
    let body_len = frame_len_unchecked(buf);
    if body_len == 0 || body_len > MAX_MESSAGE_LEN {
        return FrameScan::Desync;
    }
    let n_reports = (4 + body_len).div_ceil(REPORT_PAYLOAD);
    let total_raw = n_reports * REPORT_SIZE;
    if buf.len() < total_raw {
        return FrameScan::Incomplete;
    }
    FrameScan::Complete { len: total_raw }
}

/// Decode the payload of one complete message at the front of a raw HID report
/// buffer. Returns an owned `Vec<u8>` with report IDs and padding stripped.
#[must_use]
pub fn frame_payload(buf: &[u8]) -> Option<Vec<u8>> {
    Packet::from_bytes(buf).map(|(p, _)| p.payload)
}

/// The HID wire bytes (one [`REPORT_SIZE`] report) carrying the startup
/// handshake ([`HANDSHAKE_BODY`]).
#[must_use]
pub fn handshake_bytes() -> Vec<u8> {
    Packet::new(HANDSHAKE_BODY.to_vec()).to_bytes()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn packet_roundtrip_single_report() {
        let payload = vec![0x02, 0xDE, 0xAD, 0xBE, 0xEF];
        let bytes = Packet::new(payload.clone()).to_bytes();
        assert_eq!(bytes.len(), REPORT_SIZE);

        let (parsed, consumed) = Packet::from_bytes(&bytes).unwrap();
        assert_eq!(consumed, bytes.len());
        assert_eq!(parsed.payload, payload);
        assert_eq!(parsed.length as usize, payload.len());
    }

    #[test]
    fn to_bytes_layout_is_len_payload_no_magic() {
        let bytes = Packet::new(vec![0xAA, 0xBB]).to_bytes();
        assert_eq!(bytes[0], REPORT_ID_OUT); // report id, not a magic word
        assert_eq!(&bytes[1..5], &2u32.to_le_bytes()); // length, LE
        assert_eq!(&bytes[5..7], &[0xAA, 0xBB]); // body
        assert!(bytes[7..].iter().all(|&b| b == 0)); // zero padding
    }

    #[test]
    fn handshake_matches_known_bytes() {
        let bytes = handshake_bytes();
        assert_eq!(bytes.len(), REPORT_SIZE);
        assert_eq!(bytes[0], REPORT_ID_OUT);
        // framed handshake: [04 00 00 00][AD 10 A7 B0]
        assert_eq!(
            &bytes[1..9],
            &[0x04, 0x00, 0x00, 0x00, 0xAD, 0x10, 0xA7, 0xB0]
        );
        assert!(bytes[9..].iter().all(|&b| b == 0));
    }

    #[test]
    fn packet_roundtrip_multi_report() {
        // 600-byte body -> framed 604 -> 3 reports (3 * 256 raw bytes).
        let payload: Vec<u8> = (0..600u32).map(|i| (i % 251) as u8).collect();
        let bytes = Packet::new(payload.clone()).to_bytes();
        assert_eq!(bytes.len(), 3 * REPORT_SIZE);

        let (parsed, consumed) = Packet::from_bytes(&bytes).unwrap();
        assert_eq!(consumed, 3 * REPORT_SIZE);
        assert_eq!(parsed.payload, payload);
        assert_eq!(frame_payload(&bytes), Some(payload));
    }

    #[test]
    fn scan_frame_complete_incomplete_desync() {
        let bytes = Packet::new(vec![1, 2, 3]).to_bytes();
        assert_eq!(scan_frame(&bytes), FrameScan::Complete { len: REPORT_SIZE });

        // Fewer than one report -> Incomplete.
        assert_eq!(scan_frame(&bytes[..REPORT_SIZE - 1]), FrameScan::Incomplete);
        assert_eq!(scan_frame(&[]), FrameScan::Incomplete);

        // Multi-report message missing its last report -> Incomplete.
        let big = Packet::new(vec![7u8; 600]).to_bytes();
        assert_eq!(scan_frame(&big[..2 * REPORT_SIZE]), FrameScan::Incomplete);

        // Zero length prefix in an otherwise full report -> Desync.
        let zero = [0u8; REPORT_SIZE];
        assert_eq!(scan_frame(&zero), FrameScan::Desync);

        // Oversized length prefix -> Desync.
        let mut huge = [0u8; REPORT_SIZE];
        huge[1..5].copy_from_slice(&(MAX_MESSAGE_LEN as u32 + 1).to_le_bytes());
        assert_eq!(scan_frame(&huge), FrameScan::Desync);
    }

    #[test]
    fn report_aligned_stops_at_first_message_boundary() {
        // Two messages back to back, each whole-report aligned.
        let first = Packet::new(vec![0x02, 0xAA]).to_bytes();
        let first_len = first.len();
        let mut two = first;
        two.extend_from_slice(&Packet::new(vec![0x02, 0xBB, 0xCC]).to_bytes());

        // scan + payload see only the first message.
        assert_eq!(scan_frame(&two), FrameScan::Complete { len: first_len });
        assert_eq!(frame_payload(&two), Some(vec![0x02, 0xAA]));

        // After draining the first, the second decodes.
        let (_, consumed) = Packet::from_bytes(&two).unwrap();
        assert_eq!(
            frame_payload(&two[consumed..]),
            Some(vec![0x02, 0xBB, 0xCC])
        );
    }

    #[test]
    fn from_bytes_ignores_report_id_so_in_stream_decodes() {
        // Device->host reports carry REPORT_ID_IN; decode must still work.
        let mut bytes = Packet::new(vec![0x02, 0x11, 0x22]).to_bytes();
        bytes[0] = REPORT_ID_IN;
        assert_eq!(frame_payload(&bytes), Some(vec![0x02, 0x11, 0x22]));
    }
}
