//! Momentary two-boolean virtual button and trigger pulses.
//!
//! Many action and request properties on RØDECaster devices (such as
//! `mixLinkRequest`, `mixUnlinkRequest`, `padProgressRequestSignal`, and
//! `sipSlotCallDisconnect`) follow a momentary button-pulse pattern.
//!
//! On the wire, these properties encode two serialized booleans:
//! - Press (asserted): `(true, true)` -> `[0x01, 0x01, 0x02, 0x01, 0x01, 0x02]`
//! - Release (idle): `(false, false)` -> `[0x01, 0x01, 0x03, 0x01, 0x01, 0x03]`
//!
//! Physical interactions emit [`TriggerPhase::Press`] on touch-down and
//! [`TriggerPhase::Release`] on touch-up. Commands assert `Press`, and the
//! device resets the property with a `Release` acknowledgment.

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

/// Build a `Value::Binary` containing the `(true, true)` press payload.
pub fn press_value() -> Value {
    Value::Binary(PRESS_BYTES.to_vec())
}

/// Build a `Value::Binary` containing the `(false, false)` release payload.
pub fn release_value() -> Value {
    Value::Binary(RELEASE_BYTES.to_vec())
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
}
