//! [`PadRecorderParam`]: per-`PADRECORDER`-node parameter family.

use std::fmt;

// PADRECORDER nodes carry six properties per pad (confirmed from capture,
// firmware 1.7.3). Read-back state (Idx / State / Seconds / MemoryFull)
// plus command channels (StateRequest / Clear).
wire_param_enum! {
    /// A per-pad-recorder parameter: the flat properties on each
    /// `PADRECORDER` node. Read-back: `Idx` (which pad this recorder targets),
    /// `State` (current recording state), `Seconds` (elapsed recording
    /// duration), `MemoryFull` (out-of-space flag). Command channels:
    /// `StateRequest` (write to change recording state), `Clear` (write to
    /// erase the pad's current recording).
    PadRecorderParam {
    Idx => "padRecordIdx",
    State => "padRecordState",
    Seconds => "padRecordSeconds",
    MemoryFull => "padRecordMemoryFull",
    StateRequest => "padRecordStateRequest",
    Clear => "padRecordClear",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pad_recorder_param_round_trips_all_six() {
        for p in [
            PadRecorderParam::Idx,
            PadRecorderParam::State,
            PadRecorderParam::Seconds,
            PadRecorderParam::MemoryFull,
            PadRecorderParam::StateRequest,
            PadRecorderParam::Clear,
        ] {
            assert_eq!(PadRecorderParam::from_name(p.as_str()), p);
            assert!(p.is_known());
        }
    }
}
