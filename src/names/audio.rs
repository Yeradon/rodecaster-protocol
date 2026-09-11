//! [`AudioParam`]: the singleton `AUDIO`-node parameter family.

use std::fmt;

// The nine properties a real Pro II / Duo fullSync carries on the single
// AUDIO node (confirmed from capture, firmware 1.7.3): global audio-engine
// state: buffer size, sample rate, input/output channel counts, in/out
// latency, plus the currently-active StreamerX mix preset selector and the
// two rcSync channel-assignment / swap flags.
wire_param_enum! {
    /// A device-wide audio-engine parameter: one of the flat properties the
    /// device carries on the singleton `AUDIO` node. Covers the audio engine
    /// (buffer, sample rate, channel counts, latencies), the active
    /// StreamerX mix-preset selector, and the two rcSync channel controls.
    /// Un-typed properties arrive as [`AudioParam::Other`] for forward-compat.
    ///
    /// Firmware casing quirk preserved verbatim: `audioOuputLatency` is
    /// misspelled on the wire (missing `t` in "Output"). Consumers matching
    /// against the wire name must use this spelling.
    AudioParam {
    // StreamerX preset selector.
    ActiveStreamerXMixPreset => "activeStreamerXMixPreset",
    // Audio engine settings.
    BufferSize => "audioBufferSize",
    SampleRate => "audioSampleRate",
    InputChannels => "audioInputChannels",
    OutputChannels => "audioOutputChannels",
    InputLatency => "audioInputLatency",
    // Firmware wire name is misspelled ("Ouput" not "Output"); preserved.
    OuputLatency => "audioOuputLatency",
    // rcSync channel controls.
    RcSyncChannelAssign => "rcSyncChannelAssign",
    RcSyncChannelSwap => "rcSyncChannelSwap",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn audio_param_name_round_trips() {
        let params = [
            AudioParam::ActiveStreamerXMixPreset,
            AudioParam::BufferSize,
            AudioParam::SampleRate,
            AudioParam::InputChannels,
            AudioParam::OutputChannels,
            AudioParam::InputLatency,
            AudioParam::OuputLatency,
            AudioParam::RcSyncChannelAssign,
            AudioParam::RcSyncChannelSwap,
        ];
        for p in params {
            assert_eq!(AudioParam::from_name(p.as_str()), p);
            assert!(p.is_known());
        }
    }

    #[test]
    fn audio_param_preserves_firmware_typo() {
        // The wire misspells "Output" as "Ouput"; the crate matches it verbatim.
        assert_eq!(AudioParam::OuputLatency.as_str(), "audioOuputLatency");
        assert_ne!(
            AudioParam::from_name("audioOutputLatency"),
            AudioParam::OuputLatency
        );
    }
}
