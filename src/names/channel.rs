//! [`ChannelParam`]: the per-fader `CHANNEL`-node parameter family.

use std::fmt;

wire_param_enum! {
    /// A per-channel-strip parameter: one of the flat properties the device
    /// carries on a `CHANNEL` node (the EQ, dynamics, HPF, aphex, pan and tone
    /// controls for one fader).
    ///
    /// The preamp controls (gain, 48V, mic type) are **not** here: a source
    /// exists independently of which fader it is assigned to, so they live on
    /// the separate `INPUTSOURCE` node and are typed as [`InputSourceParam`].
    /// `CHANNEL` only references its source via `channelInputSource`.
    ChannelParam {
    // EQ (3-band parametric).
    EqOn => "eqOn",
    EqLowOn => "eqLowOn",
    EqLowGain => "eqLowGain",
    EqLowQ => "eqLowQ",
    EqLowShelf => "eqLowShelf",
    EqMidOn => "eqMidOn",
    EqMidGain => "eqMidGain",
    EqMidQ => "eqMidQ",
    EqMidBell => "eqMidBell",
    EqHighOn => "eqHighOn",
    EqHighGain => "eqHighGain",
    EqHighQ => "eqHighQ",
    EqHighBell => "eqHighBell",
    // NOTE: eqParamMode{Low,Mid,High} are intentionally NOT here. A real
    // fullSync shows them on the `GUI` node (which EQ band the touchscreen is
    // editing), not on `CHANNEL`. They are UI state, not a per-strip parameter.
    // Compressor.
    CompressorOn => "compressorOn",
    CompressorThreshold => "compressorThreshold",
    CompressorRatio => "compressorRatio",
    CompressorAttack => "compressorAttack",
    CompressorRelease => "compressorRelease",
    CompressorGain => "compressorGain",
    // De-esser.
    DeesserOn => "deesserOn",
    DeesserThreshold => "deesserThreshold",
    DeesserRatio => "deesserRatio",
    DeesserAttack => "deesserAttack",
    DeesserRelease => "deesserRelease",
    DeesserGain => "deesserGain",
    DeesserFrequency => "deesserFrequency",
    // Noise gate.
    NoiseGateOn => "noiseGateOn",
    NoiseGateThreshold => "noiseGateThreshold",
    NoiseGateRange => "noiseGateRange",
    NoiseGateAttack => "noiseGateAttack",
    NoiseGateHold => "noiseGateHold",
    NoiseGateRelease => "noiseGateRelease",
    NoiseGateHysteresis => "noiseGateHysteresis",
    // High-pass filter.
    HpfOn => "hpfOn",
    HpfFrequency => "hpfFrequency",
    HpfSlope => "hpfSlope",
    HpfLowerOn => "hpfLowerOn",
    HpfHigherOn => "hpfHigherOn",
    // Aphex (Aural Exciter + Big Bottom).
    AphexOn => "aphexOn",
    AphexAeMix => "aphexAEMix",
    AphexAeTune => "aphexAETune",
    AphexBbDrive => "aphexBBDrive",
    AphexBbTune => "aphexBBTune",
    // Pan.
    PanOn => "channelPanOn",
    Pan => "channelPan",
    PanL => "channelPanL",
    PanR => "channelPanR",
    PanMode => "channelPanMode",
    // RODE voice tone (the "Big / Bright" sliders).
    Depth => "channelDepth",
    Sparkle => "channelSparkle",
    Punch => "channelPunch",
    // NOTE: the input* preamp params (inputType, inputPower, inputMicrophoneGain,
    // inputColour, ...) are intentionally NOT here. A real fullSync proves they
    // live on the separate `INPUTSOURCE` node (one per source), not on `CHANNEL`.
    // CHANNEL only references a source via `channelInputSource`. The preamp
    // controls are typed as [`InputSourceParam`] and addressed by [`Source`].
    // Channel-level flags.
    AdvancedProcessing => "channelAdvancedProcessing",
    BypassProcessing => "channelBypassProcessing",
    CurrentFxPreset => "channelCurrentFxPreset",
    ListenSource => "channelListenSource",
    TalkbackEnable => "channelTalkbackEnable",
    WirelessMute => "channelWirelessMute",
    ChannelIndex => "channelIndex",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn channel_param_name_round_trips() {
        // A representative slice of every family: from_name(as_str(x)) == x.
        let params = [
            ChannelParam::EqHighGain,
            ChannelParam::CompressorThreshold,
            ChannelParam::DeesserFrequency,
            ChannelParam::NoiseGateOn,
            ChannelParam::HpfSlope,
            ChannelParam::AphexAeMix,
            ChannelParam::PanMode,
            ChannelParam::Depth,
            ChannelParam::ListenSource,
            ChannelParam::WirelessMute,
        ];
        for p in params {
            assert_eq!(ChannelParam::from_name(p.as_str()), p);
            assert!(p.is_known());
            // Display matches the wire name.
            assert_eq!(p.to_string(), p.as_str());
        }
    }

    #[test]
    fn channel_param_exact_wire_names() {
        // Variant spelling differs from the wire name for acronym cases; pin a
        // few exact wire strings so a rename can't silently change the protocol.
        assert_eq!(ChannelParam::AphexAeMix.as_str(), "aphexAEMix");
        assert_eq!(ChannelParam::AphexBbDrive.as_str(), "aphexBBDrive");
        assert_eq!(ChannelParam::Pan.as_str(), "channelPan");
        assert_eq!(ChannelParam::Depth.as_str(), "channelDepth");
    }

    #[test]
    fn channel_param_unknown_falls_back_to_other() {
        let p = ChannelParam::from_name("channelMysteryKnob");
        assert_eq!(p, ChannelParam::Other("channelMysteryKnob".to_string()));
        assert!(!p.is_known());
        // Other round-trips its raw name through as_str/from_name too.
        assert_eq!(p.as_str(), "channelMysteryKnob");
        assert_eq!(ChannelParam::from_name(p.as_str()), p);
    }
}
