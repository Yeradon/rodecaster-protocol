//! [`OutputParam`]: the singleton `OUTPUT`-node parameter family.

// The fifteen properties a real Pro II / Duo fullSync carries on the single
// OUTPUT node (confirmed from capture, firmware 1.7.3): monitor (speaker) and
// Bluetooth output levels/mutes, the multi-out mode, and the recording-bus
// flags the firmware co-locates here.
wire_param_enum! {
    /// An output-bus parameter: one of the flat properties the device carries on
    /// the single `OUTPUT` node. There is exactly one, so this family takes no
    /// addressing key. Covers the monitor (speaker) output, the Bluetooth
    /// output, the multi-output mode, and the recording-bus flags the firmware
    /// co-locates on this node.
    OutputParam {
    // Monitor (speaker) output.
    MonLevel => "outputMonLevel",
    MonMute => "outputMonMute",
    MonAutoMute => "outputMonAutoMute",
    MonAutoMuteActive => "outputMonAutoMuteActive",
    MonFixed => "outputMonFixed",
    // Bluetooth output.
    BtLevel => "outputBTLevel",
    BtMute => "outputBTMute",
    BtAutoMute => "outputBTAutoMute",
    BtAutoMuteActive => "outputBTAutoMuteActive",
    // Multi-output (USB/aux fan-out).
    MultiMode => "outputMultiMode",
    MultiBypass => "outputMultiBypass",
    Prefader => "outputPrefader",
    // Recording bus (the firmware carries these on OUTPUT).
    RecordingCompressionQuality => "recordingCompressionQuality",
    RecordingMultitrackMode => "recordingMultitrackMode",
    RecordingProcessingBypass => "recordingProcessingBypass",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn output_param_name_round_trips() {
        // Every typed OUTPUT param: from_name(as_str(x)) == x.
        let params = [
            OutputParam::MonLevel,
            OutputParam::MonMute,
            OutputParam::MonAutoMute,
            OutputParam::MonAutoMuteActive,
            OutputParam::MonFixed,
            OutputParam::BtLevel,
            OutputParam::BtMute,
            OutputParam::BtAutoMute,
            OutputParam::BtAutoMuteActive,
            OutputParam::MultiMode,
            OutputParam::MultiBypass,
            OutputParam::Prefader,
            OutputParam::RecordingCompressionQuality,
            OutputParam::RecordingMultitrackMode,
            OutputParam::RecordingProcessingBypass,
        ];
        for p in params {
            assert_eq!(OutputParam::from_name(p.as_str()), p);
            assert!(p.is_known());
            assert_eq!(p.to_string(), p.as_str());
        }
    }

    #[test]
    fn output_param_exact_wire_names() {
        // The BT acronym is upper-case on the wire; recording* params live on
        // OUTPUT but keep their own prefix.
        assert_eq!(OutputParam::BtLevel.as_str(), "outputBTLevel");
        assert_eq!(OutputParam::MonLevel.as_str(), "outputMonLevel");
        assert_eq!(
            OutputParam::RecordingMultitrackMode.as_str(),
            "recordingMultitrackMode"
        );
    }

    #[test]
    fn output_param_unknown_falls_back_to_other() {
        let p = OutputParam::from_name("outputMysteryFlag");
        assert_eq!(p, OutputParam::Other("outputMysteryFlag".to_string()));
        assert!(!p.is_known());
        assert_eq!(OutputParam::from_name(p.as_str()), p);
    }
}
