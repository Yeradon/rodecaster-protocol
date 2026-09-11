//! [`TestParam`]: the singleton `TEST`-node parameter family.

use std::fmt;

// The two properties the TEST node carries (confirmed from capture, firmware
// 1.7.3). Diagnostic / factory-test channels: writing `AllLedsWhite` turns
// every front-panel LED white for hardware inspection; `ToneGeneration`
// selects an internal test tone.
wire_param_enum! {
    /// A device-diagnostic parameter: the two flat properties on the
    /// singleton `TEST` node. `AllLedsWhite` is a factory-test command
    /// (writing `Int(1)` engages the all-LEDs-white mode; verified on Duo
    /// fw 1.7.3, write persists across refresh); `ToneGeneration` selects
    /// an internal test-tone source for audio-path verification. Both wire
    /// types are `Int`; the crate types the property name only.
    ///
    /// Firmware wire name preserves the capitalized "LEDS" (`allLEDSWhite`).
    TestParam {
    AllLedsWhite => "allLEDSWhite",
    ToneGeneration => "toneGeneration",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_param_round_trips() {
        for p in [TestParam::AllLedsWhite, TestParam::ToneGeneration] {
            assert_eq!(TestParam::from_name(p.as_str()), p);
            assert!(p.is_known());
        }
    }

    #[test]
    fn test_param_preserves_leds_capitalization() {
        assert_eq!(TestParam::AllLedsWhite.as_str(), "allLEDSWhite");
    }
}
