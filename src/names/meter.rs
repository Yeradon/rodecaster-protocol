//! [`MeterParam`]: per-`METER`-node parameter family.

use std::fmt;

// The six properties each METER node carries (confirmed from capture,
// firmware 1.7.3): a passthrough fader-level snapshot and left/right meter
// level + peak values, plus a `meterStereo` flag distinguishing stereo pairs.
// Note that `faderLevel` also appears on `FADER` nodes under
// `PHYSICALINTERFACE`; the two are separate wire properties even though
// they share a name (path shape distinguishes them at decode time).
wire_param_enum! {
    /// A per-meter parameter: one of the flat properties the device carries
    /// on each `METER` node (typically one per fader strip, though the exact
    /// count depends on firmware layout). Provides live L/R meter levels and
    /// peaks plus a stereo-pair flag. `FaderLevel` is a passthrough of the
    /// strip's fader position (the same value that appears on the strip's
    /// `FADER` node under `PHYSICALINTERFACE`).
    ///
    /// The `faderLevel` wire name is shared with `FADER`; decoding
    /// distinguishes by path shape (METER paths never match FADER paths).
    MeterParam {
    FaderLevel => "faderLevel",
    MeterLevelL => "meterLevelL",
    MeterLevelR => "meterLevelR",
    MeterPeakL => "meterPeakL",
    MeterPeakR => "meterPeakR",
    MeterStereo => "meterStereo",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn meter_param_name_round_trips() {
        let params = [
            MeterParam::FaderLevel,
            MeterParam::MeterLevelL,
            MeterParam::MeterLevelR,
            MeterParam::MeterPeakL,
            MeterParam::MeterPeakR,
            MeterParam::MeterStereo,
        ];
        for p in params {
            assert_eq!(MeterParam::from_name(p.as_str()), p);
            assert!(p.is_known());
        }
    }

    #[test]
    fn meter_param_shares_fader_level_wire_name() {
        // `faderLevel` intentionally collides with FADER's property — path
        // shape distinguishes the two at decode time.
        assert_eq!(MeterParam::FaderLevel.as_str(), "faderLevel");
    }
}
