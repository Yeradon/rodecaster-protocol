//! [`RecordingsParam`] + [`RecordingParam`]: the recordings container + per-
//! recording parameter families.

// The three properties a real Pro II / Duo fullSync carries on the singleton
// RECORDINGS container node (confirmed from capture, firmware 1.7.3): summary
// counts + a request-to-delete command channel.
wire_param_enum! {
    /// A recordings-container parameter: one of the flat properties the device
    /// carries on the singleton `RECORDINGS` node (the parent of individual
    /// `RECORDING` child nodes). Read-back state summarizes the collection
    /// (total count + total duration seconds). `RequestDeleteUid` is the write
    /// channel that removes a recording by its `RecordingParam::Uid`.
    RecordingsParam {
    TotalCount => "recordingTotalCount",
    TotalDuration => "recordingTotalDuration",
    RequestDeleteUid => "requestDeleteUID",
    }
}

// The two properties a real Pro II / Duo fullSync carries on each RECORDING
// child node (confirmed from capture, firmware 1.7.3). `Content` is a pipe-
// separated metadata string: `name|hash|path|timestampMs|durationSec|flags...`.
// `Uid` is a stable integer identifier used by `RecordingsParam::RequestDeleteUid`.
wire_param_enum! {
    /// A per-recording parameter: one of the two flat properties the device
    /// carries on each `RECORDING` child node under the `RECORDINGS`
    /// container. `Content` arrives as a pipe-separated field string
    /// (`name|hash|path|timestampMs|durationSec|flags...`); the crate types
    /// the property name and leaves field parsing to the caller.
    RecordingParam {
    Content => "recordingContent",
    Uid => "recordingUID",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recordings_param_name_round_trips() {
        let params = [
            RecordingsParam::TotalCount,
            RecordingsParam::TotalDuration,
            RecordingsParam::RequestDeleteUid,
        ];
        for p in params {
            assert_eq!(RecordingsParam::from_name(p.as_str()), p);
            assert!(p.is_known());
        }
    }

    #[test]
    fn recording_param_name_round_trips() {
        let params = [RecordingParam::Content, RecordingParam::Uid];
        for p in params {
            assert_eq!(RecordingParam::from_name(p.as_str()), p);
            assert!(p.is_known());
        }
    }

    #[test]
    fn recording_param_preserves_firmware_casing() {
        assert_eq!(RecordingParam::Uid.as_str(), "recordingUID");
        assert_eq!(
            RecordingsParam::RequestDeleteUid.as_str(),
            "requestDeleteUID"
        );
    }
}
