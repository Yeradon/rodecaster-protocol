//! [`EffectsParam`]: the per-slot root `EFFECTS_PARAMETERS`-node parameter family.

use std::fmt;

// The (up to) twenty-one properties a real Pro II / Duo fullSync carries on each
// root-level EFFECTS_PARAMETERS node (confirmed from capture, firmware 1.7.3).
// The device exposes a contiguous run of these at the tree root, one per
// channel-strip effects slot, with `effectsIdx` equal to the run ordinal, so
// address a slot by its index. (The PADEFFECTS node nests its own
// EFFECTS_PARAMETERS set with non-contiguous indices; those pad/sample effects
// are a separate addressing context, not covered here.)
wire_param_enum! {
    /// A per-slot effects parameter: one of the flat properties the device
    /// carries on a root `EFFECTS_PARAMETERS` node. The device exposes a
    /// contiguous run of these (one per channel-strip effects slot), so address a
    /// slot by its index. Covers the six processors the Rodecaster effects engine
    /// exposes (reverb, echo/delay, pitch shift, distortion, robot, voice
    /// disguise), each with its `*On` enable plus its tone controls. `effectsIdx`
    /// echoes the slot index; `channelInputSource` (present only when the slot is
    /// bound) is the source the slot processes.
    EffectsParam {
    // Reverb.
    ReverbOn => "reverbOn",
    ReverbMix => "reverbMix",
    ReverbModel => "reverbModel",
    ReverbHighCut => "reverbHighCut",
    ReverbLowCut => "reverbLowCut",
    // Echo / delay.
    EchoOn => "echoOn",
    EchoMix => "echoMix",
    EchoDecay => "echoDecay",
    EchoDelay => "echoDelay",
    EchoHighCut => "echoHighCut",
    EchoLowCut => "echoLowCut",
    // Pitch shift.
    PitchShiftOn => "pitchShiftOn",
    PitchShiftSemitones => "pitchShiftSemitones",
    // Distortion.
    DistortionOn => "distortionOn",
    DistortionIntensity => "distortionIntensity",
    // Robot.
    RobotOn => "robotOn",
    RobotLevel => "robotLevel",
    RobotMix => "robotMix",
    // Voice disguise.
    VoiceDisguiseOn => "voiceDisguiseOn",
    // Identity / binding.
    Idx => "effectsIdx",
    InputSource => "channelInputSource",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn effects_param_round_trips_and_exact_wire_names() {
        for p in [
            EffectsParam::ReverbOn,
            EffectsParam::ReverbMix,
            EffectsParam::EchoDelay,
            EffectsParam::PitchShiftSemitones,
            EffectsParam::DistortionIntensity,
            EffectsParam::RobotMix,
            EffectsParam::VoiceDisguiseOn,
            EffectsParam::Idx,
            EffectsParam::InputSource,
        ] {
            assert_eq!(EffectsParam::from_name(p.as_str()), p);
            assert!(p.is_known());
            assert_eq!(p.to_string(), p.as_str());
        }
        assert_eq!(EffectsParam::ReverbModel.as_str(), "reverbModel");
        assert_eq!(EffectsParam::EchoHighCut.as_str(), "echoHighCut");
        assert_eq!(EffectsParam::Idx.as_str(), "effectsIdx");
        // The source binding shares its wire name with the channel strip's source
        // assignment; which family it resolves to is decided by the node path,
        // not the property name.
        assert_eq!(EffectsParam::InputSource.as_str(), "channelInputSource");
    }

    #[test]
    fn effects_param_unknown_falls_back_to_other() {
        let p = EffectsParam::from_name("flangerOn");
        assert_eq!(p, EffectsParam::Other("flangerOn".to_string()));
        assert!(!p.is_known());
        assert_eq!(EffectsParam::from_name(p.as_str()), p);
    }
}
