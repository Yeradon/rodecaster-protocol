//! Momentary two-boolean virtual button and trigger pulses.
//!
//! Many action and request properties on RØDECaster devices (such as
//! `mixLinkRequest`, `mixUnlinkRequest`, `padProgressRequestSignal`, and
//! `sipSlotCallDisconnect`) follow a momentary button-pulse pattern.
//!
//! In JUCE UI architecture, button interactions are governed by two boolean
//! flags: `isDown` (button actively pressed down) and `isOver` (pointer or
//! touch focused over the button). When a user taps a virtual button:
//! 1. Touch Down: both `isDown` and `isOver` are asserted -> `(true, true)`
//! 2. Touch Up / Reset: both are released -> `(false, false)`
//!
//! On the wire, JUCE ValueTree frames this property as a `Binary` (tag `0x08`)
//! containing two consecutive serialized `juce::var::Bool` values:
//!
//! - Press (touch-down / asserted): `(true, true)` -> `[0x01, 0x01, 0x02, 0x01, 0x01, 0x02]`
//! - Release (touch-up / idle): `(false, false)` -> `[0x01, 0x01, 0x03, 0x01, 0x01, 0x03]`
//!
//! Each serialized boolean follows standard JUCE encoding:
//! - `0x01, 0x01`: `writeCompressedInt(1)` (1 payload byte follows)
//! - `0x02` / `0x03`: `varMarker_BoolTrue` (`0x02`) or `varMarker_BoolFalse` (`0x03`)
//!
//! On physical touchscreen interactions, the hardware emits two consecutive events:
//! [`TriggerPhase::Press`] on touch-down, followed by [`TriggerPhase::Release`] on
//! touch-up. When commanded over USB or TCP, the client asserts `Press`, and the
//! firmware executes the action and resets the property with a `Release` echo.

use crate::juce_var::{marker, Value};

/// Encode two booleans into the canonical 6-byte JUCE payload.
pub const fn bool_pair(a: bool, b: bool) -> [u8; 6] {
    let byte_a = if a {
        marker::BOOL_TRUE
    } else {
        marker::BOOL_FALSE
    };
    let byte_b = if b {
        marker::BOOL_TRUE
    } else {
        marker::BOOL_FALSE
    };
    [0x01, 0x01, byte_a, 0x01, 0x01, byte_b]
}

/// The 6-byte trigger payload when asserting a press: `(true, true)`.
pub const PRESS_BYTES: [u8; 6] = bool_pair(true, true);

/// The 6-byte trigger payload when releasing / idle: `(false, false)`.
pub const RELEASE_BYTES: [u8; 6] = bool_pair(false, false);

/// Alias for [`PRESS_BYTES`].
pub const REQUEST_BYTES: [u8; 6] = PRESS_BYTES;

/// Alias for [`RELEASE_BYTES`].
pub const ACK_BYTES: [u8; 6] = RELEASE_BYTES;

/// Build a `Value::Binary` containing the `(true, true)` press payload.
pub fn press_value() -> Value {
    Value::Binary(PRESS_BYTES.to_vec())
}

/// Build a `Value::Binary` containing the `(false, false)` release payload.
pub fn release_value() -> Value {
    Value::Binary(RELEASE_BYTES.to_vec())
}

/// Alias for [`press_value`].
pub fn request_value() -> Value {
    press_value()
}

/// Alias for [`release_value`].
pub fn ack_value() -> Value {
    release_value()
}

/// Decode a 6-byte payload into its two boolean values.
///
/// Returns `None` if the payload does not match the 6-byte two-boolean JUCE format.
pub fn decode_bool_pair(bytes: &[u8]) -> Option<(bool, bool)> {
    if bytes.len() != 6 {
        return None;
    }
    if bytes[0] != 0x01 || bytes[1] != 0x01 || bytes[3] != 0x01 || bytes[4] != 0x01 {
        return None;
    }
    let a = match bytes[2] {
        marker::BOOL_TRUE => true,
        marker::BOOL_FALSE => false,
        _ => return None,
    };
    let b = match bytes[5] {
        marker::BOOL_TRUE => true,
        marker::BOOL_FALSE => false,
        _ => return None,
    };
    Some((a, b))
}

/// Phase of a momentary button press or trigger pulse.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TriggerPhase {
    /// Button pressed down / trigger asserted (`(true, true)`).
    Press,
    /// Button released / trigger idle (`(false, false)`).
    Release,
}

#[allow(non_upper_case_globals)]
impl TriggerPhase {
    /// Legacy alias for [`TriggerPhase::Press`].
    pub const ClientTrigger: Self = Self::Press;
    /// Legacy alias for [`TriggerPhase::Release`].
    pub const DeviceAck: Self = Self::Release;
}

/// Backward-compatible type alias for [`TriggerPhase`].
pub type TriggerOrigin = TriggerPhase;

/// Decode the trigger phase from a `Value`.
///
/// Returns `Some(TriggerPhase)` if the value is a valid 6-byte two-boolean payload.
pub fn decode_phase(value: &Value) -> Option<TriggerPhase> {
    let bytes = match value {
        Value::Binary(b) => b,
        _ => return None,
    };
    let (a, _b) = decode_bool_pair(bytes)?;
    if a {
        Some(TriggerPhase::Press)
    } else {
        Some(TriggerPhase::Release)
    }
}

/// Backward-compatible alias for [`decode_phase`].
pub fn decode_origin(value: &Value) -> Option<TriggerPhase> {
    decode_phase(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bool_pair_encoding() {
        assert_eq!(PRESS_BYTES, [0x01, 0x01, 0x02, 0x01, 0x01, 0x02]);
        assert_eq!(RELEASE_BYTES, [0x01, 0x01, 0x03, 0x01, 0x01, 0x03]);
    }

    #[test]
    fn bool_pair_round_trips() {
        assert_eq!(decode_bool_pair(&PRESS_BYTES), Some((true, true)));
        assert_eq!(decode_bool_pair(&RELEASE_BYTES), Some((false, false)));
        assert_eq!(
            decode_bool_pair(&bool_pair(true, false)),
            Some((true, false))
        );
        assert_eq!(
            decode_bool_pair(&bool_pair(false, true)),
            Some((false, true))
        );
    }

    #[test]
    fn decode_phase_semantics() {
        assert_eq!(decode_phase(&press_value()), Some(TriggerPhase::Press));
        assert_eq!(decode_phase(&release_value()), Some(TriggerPhase::Release));
        assert_eq!(decode_phase(&Value::Binary(vec![0x01, 0x01, 0x02])), None);
        assert_eq!(decode_phase(&Value::Bool(true)), None);
    }

    #[test]
    fn backwards_compatible_aliases() {
        assert_eq!(TriggerOrigin::ClientTrigger, TriggerPhase::Press);
        assert_eq!(TriggerOrigin::DeviceAck, TriggerPhase::Release);
        assert_eq!(
            decode_origin(&request_value()),
            Some(TriggerOrigin::ClientTrigger)
        );
        assert_eq!(decode_origin(&ack_value()), Some(TriggerOrigin::DeviceAck));
    }
}
