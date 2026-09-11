//! [`PadParam`]: the per-pad `PAD`-node parameter family (nested under `SOUNDPADS`).

// The forty-eight properties a real Pro II / Duo fullSync carries on each `PAD`
// node (confirmed from capture, firmware 1.7.3). The `PAD` nodes are nested
// children of the single root `SOUNDPADS` node (one per sound pad), with `padIdx`
// equal to the child ordinal, so a pad is addressed by its index on a two-level
// path (like `FADER` under `PHYSICALINTERFACE`). These cover the whole sound-pad
// surface: the loaded sample + playback envelope, the per-pad mixer routing
// (back-channel sends), the trigger/MIDI mapping, the SIP/phone-book binding, and
// the effects input.
wire_param_enum! {
    /// A sound-pad parameter: one of the flat properties the device carries on
    /// one `PAD` node (nested under the single root `SOUNDPADS` node, one per
    /// pad). Pads are addressed by index; `padIdx` echoes that index. Covers the
    /// loaded sample + playback envelope, per-pad mixer routing (the
    /// `padMixerBackChannel*` sends), the trigger / MIDI mapping
    /// (`padTrigger*`), the SIP / phone-book binding (`padSIP*`), gain, colour
    /// and the effects input.
    ///
    /// `padProgressRequestSignal` is a request/command pulse (a JUCE binary blob),
    /// not a scalar; it rides byte-faithfully like any other value.
    PadParam {
    // Identity / appearance.
    Idx => "padIdx",
    ColourIndex => "padColourIndex",
    Name => "padName",
    Type => "padType",
    IsInternal => "padIsInternal",
    RcvSyncPadType => "padRCVSyncPadType",
    // Loaded sample + transport.
    FilePath => "padFilePath",
    Active => "padActive",
    Loop => "padLoop",
    Replay => "padReplay",
    PlayMode => "padPlayMode",
    Progress => "padProgress",
    ProgressRequestSignal => "padProgressRequestSignal",
    Gain => "padGain",
    // Playback envelope (in/out + fades).
    EnvStart => "padEnvStart",
    EnvStop => "padEnvStop",
    EnvFadeIn => "padEnvFadeIn",
    EnvFadeOut => "padEnvFadeOut",
    // Per-pad mixer mode + censor + fades.
    MixerMode => "padMixerMode",
    MixerTriggerMode => "padMixerTriggerMode",
    MixerCensorCustom => "padMixerCensorCustom",
    MixerCensorFilePath => "padMixerCensorFilePath",
    MixerFadeInSeconds => "padMixerFadeInSeconds",
    MixerFadeOutSeconds => "padMixerFadeOutSeconds",
    MixerFadeExcludeHost => "padMixerFadeExcludeHost",
    // Per-pad mixer back-channel sends.
    MixerBackChannelMic2 => "padMixerBackChannelMic2",
    MixerBackChannelMic3 => "padMixerBackChannelMic3",
    MixerBackChannelMic4 => "padMixerBackChannelMic4",
    MixerBackChannelUsb1Comms => "padMixerBackChannelUsb1Comms",
    MixerBackChannelUsb2Main => "padMixerBackChannelUsb2Main",
    MixerBackChannelBluetooth => "padMixerBackChannelBluetooth",
    MixerBackChannelCallMe1 => "padMixerBackChannelCallMe1",
    MixerBackChannelCallMe2 => "padMixerBackChannelCallMe2",
    MixerBackChannelCallMe3 => "padMixerBackChannelCallMe3",
    // Effects input.
    EffectInput => "padEffectInput",
    EffectTriggerMode => "padEffectTriggerMode",
    // SIP / phone-book binding.
    SipPhoneBookEntry => "padSIPPhoneBookEntry",
    SipCallSlot => "padSIPCallSlot",
    SipFlashState => "padSIPFlashState",
    SipQdLock => "padSIPQdLock",
    // Trigger / MIDI mapping.
    TriggerMode => "padTriggerMode",
    TriggerSend => "padTriggerSend",
    TriggerType => "padTriggerType",
    TriggerCustom => "padTriggerCustom",
    TriggerControl => "padTriggerControl",
    TriggerChannel => "padTriggerChannel",
    TriggerOn => "padTriggerOn",
    TriggerOff => "padTriggerOff",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pad_param_round_trips_and_exact_wire_names() {
        for p in [
            PadParam::Idx,
            PadParam::ColourIndex,
            PadParam::Name,
            PadParam::Type,
            PadParam::FilePath,
            PadParam::Active,
            PadParam::Loop,
            PadParam::PlayMode,
            PadParam::Gain,
            PadParam::MixerMode,
            PadParam::EffectInput,
            PadParam::SipPhoneBookEntry,
            PadParam::TriggerMode,
        ] {
            assert_eq!(PadParam::from_name(p.as_str()), p);
            assert!(p.is_known());
            assert_eq!(p.to_string(), p.as_str());
        }
        // Spot-check the casing that camel-casing the variant name would get wrong.
        assert_eq!(PadParam::RcvSyncPadType.as_str(), "padRCVSyncPadType");
        assert_eq!(
            PadParam::MixerBackChannelUsb1Comms.as_str(),
            "padMixerBackChannelUsb1Comms"
        );
        assert_eq!(PadParam::SipPhoneBookEntry.as_str(), "padSIPPhoneBookEntry");
        assert_eq!(PadParam::SipQdLock.as_str(), "padSIPQdLock");
    }

    #[test]
    fn pad_param_unknown_falls_back_to_other() {
        let p = PadParam::from_name("padMysteryFlag");
        assert_eq!(p, PadParam::Other("padMysteryFlag".to_string()));
        assert!(!p.is_known());
        assert_eq!(PadParam::from_name(p.as_str()), p);
    }
}
