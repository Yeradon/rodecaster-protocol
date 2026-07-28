//! [`InputSourceParam`]: the per-source `INPUTSOURCE`-node parameter family.

use std::fmt;

// The twelve properties a real Pro II / Duo fullSync carries on every
// INPUTSOURCE node (confirmed from capture, firmware 1.7.3).
wire_param_enum! {
    /// A per-input-source parameter: one of the flat properties the device
    /// carries on an `INPUTSOURCE` node (preamp gain, power/48V, mic type,
    /// phase, colour, wireless serial, SIP/RCV routing).
    ///
    /// These are **not** channel-strip params: a source exists independently of
    /// which fader (if any) it is assigned to, and a real fullSync shows every
    /// input* property on `INPUTSOURCE`, never on `CHANNEL`. Address an input
    /// source by [`crate::Source`] (the source ordinal == `inputId`).
    InputSourceParam {
    InputId => "inputId",
    InputColour => "inputColour",
    InputType => "inputType",
    InputPower => "inputPower",
    InputMicrophoneType => "inputMicrophoneType",
    InputMicrophoneGain => "inputMicrophoneGain",
    InputDigitalGain => "inputDigitalGain",
    InputInstrumentGain => "inputInstrumentGain",
    InputPhaseFlip => "inputPhaseFlip",
    InputWirelessSn => "inputWirelessSN",
    InputSipCallSlot => "inputSipCallSlot",
    InputRcvAudioSourceType => "inputRcvAudioSourceType",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn input_source_param_name_round_trips() {
        // Every typed INPUTSOURCE param: from_name(as_str(x)) == x.
        let params = [
            InputSourceParam::InputId,
            InputSourceParam::InputColour,
            InputSourceParam::InputType,
            InputSourceParam::InputPower,
            InputSourceParam::InputMicrophoneType,
            InputSourceParam::InputMicrophoneGain,
            InputSourceParam::InputDigitalGain,
            InputSourceParam::InputInstrumentGain,
            InputSourceParam::InputPhaseFlip,
            InputSourceParam::InputWirelessSn,
            InputSourceParam::InputSipCallSlot,
            InputSourceParam::InputRcvAudioSourceType,
        ];
        for p in params {
            assert_eq!(InputSourceParam::from_name(p.as_str()), p);
            assert!(p.is_known());
            assert_eq!(p.to_string(), p.as_str());
        }
    }

    #[test]
    fn input_source_param_exact_wire_names() {
        // Acronym casing must match the firmware exactly.
        assert_eq!(
            InputSourceParam::InputWirelessSn.as_str(),
            "inputWirelessSN"
        );
        assert_eq!(
            InputSourceParam::InputRcvAudioSourceType.as_str(),
            "inputRcvAudioSourceType"
        );
        assert_eq!(InputSourceParam::InputId.as_str(), "inputId");
    }

    #[test]
    fn input_source_param_unknown_falls_back_to_other() {
        let p = InputSourceParam::from_name("inputMysteryFlag");
        assert_eq!(p, InputSourceParam::Other("inputMysteryFlag".to_string()));
        assert!(!p.is_known());
        assert_eq!(p.as_str(), "inputMysteryFlag");
        assert_eq!(InputSourceParam::from_name(p.as_str()), p);
    }
}
