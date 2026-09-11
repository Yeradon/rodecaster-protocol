//! [`RecorderParam`]: the singleton `RECORDER`-node parameter family.

// The five properties a real Pro II / Duo fullSync carries on the single
// RECORDER node (confirmed from capture, firmware 1.7.3): the multitrack
// recorder's transport state and the request-to-change-state command channel.
wire_param_enum! {
    /// A recorder parameter: one of the flat properties the device carries on
    /// the single `RECORDER` node. There is exactly one recorder, so this family
    /// takes no addressing key. The `record*` properties are read-back state
    /// (current transport, elapsed ms, byte rate); the `request*` properties are
    /// the write channel (set `requestRecordState` to start/stop recording, set
    /// `requestDropMarker` to drop a chapter marker).
    RecorderParam {
    // Read-back state.
    RecordState => "recordState",
    RecordTimeMs => "recordTimeMs",
    RecordBytesPerSecond => "recordBytesPerSecond",
    // Write channel (commands).
    RequestRecordState => "requestRecordState",
    RequestDropMarker => "requestDropMarker",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recorder_param_name_round_trips() {
        let params = [
            RecorderParam::RecordState,
            RecorderParam::RecordTimeMs,
            RecorderParam::RecordBytesPerSecond,
            RecorderParam::RequestRecordState,
            RecorderParam::RequestDropMarker,
        ];
        for p in params {
            assert_eq!(RecorderParam::from_name(p.as_str()), p);
            assert!(p.is_known());
            assert_eq!(p.to_string(), p.as_str());
        }
    }

    #[test]
    fn recorder_param_separates_state_from_command_channel() {
        // Read-back state vs the request* write channel keep distinct wire names.
        assert_eq!(RecorderParam::RecordState.as_str(), "recordState");
        assert_eq!(
            RecorderParam::RequestRecordState.as_str(),
            "requestRecordState"
        );
        assert_eq!(
            RecorderParam::RequestDropMarker.as_str(),
            "requestDropMarker"
        );
    }

    #[test]
    fn recorder_param_unknown_falls_back_to_other() {
        let p = RecorderParam::from_name("recordMystery");
        assert_eq!(p, RecorderParam::Other("recordMystery".to_string()));
        assert!(!p.is_known());
        assert_eq!(RecorderParam::from_name(p.as_str()), p);
    }
}
