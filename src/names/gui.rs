//! [`GuiParam`]: the singleton root `GUI`-node front-panel UI-state family.

use std::fmt;

// The fourteen properties a real Pro II / Duo fullSync carries on the single
// root-level GUI node (confirmed from capture, firmware 1.7.3). GUI is the
// front-panel / touchscreen UI-state singleton: display + button brightness,
// the active pad bank, the metering mode, and which EQ band the touchscreen is
// editing. None of it is audio routing.
wire_param_enum! {
    /// A front-panel UI parameter: one of the flat properties the device carries
    /// on the single root `GUI` node. This is touchscreen / display state, not
    /// audio routing: screen + button brightness, the selected pad bank, the
    /// metering mode, and which EQ band the touchscreen EQ view is focused on.
    ///
    /// Two wire-name subtleties worth calling out:
    /// - `screenTouched` here is the device's *readback* flag (the UI reporting
    ///   the screen is being touched). It is distinct from
    ///   [`crate::Command::ScreenTouched`], which is a separate "wake the
    ///   display" pulse frame, not a write to this property.
    /// - `eqParamModeLow` / `eqParamModeMid` / `eqParamModeHigh` live on `GUI`,
    ///   not on the channel strip: they select which EQ band the touchscreen is
    ///   editing (UI focus), so they are intentionally absent from
    ///   [`ChannelParam`].
    GuiParam {
    // Language.
    Lang => "lang",
    // Display / button brightness + dimming.
    ScreenBrightness => "screenBrightness",
    AutoBrightness => "autoBrightness",
    ScreenDimAfterSeconds => "screenDimAfterSeconds",
    ActiveButtonsBrightness => "activeButtonsBrightness",
    InactiveButtonsBrightness => "inactiveButtonsBrightness",
    // Metering.
    Metering => "metering",
    BroadcastMeters => "broadcastMeters",
    // Pad bank / edit state.
    SelectedBank => "selectedBank",
    PadActiveEdit => "padActiveEdit",
    ScreenTouched => "screenTouched",
    // Touchscreen EQ-band focus (UI state, not channel-strip EQ).
    EqParamModeLow => "eqParamModeLow",
    EqParamModeMid => "eqParamModeMid",
    EqParamModeHigh => "eqParamModeHigh",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gui_param_round_trips_and_exact_wire_names() {
        for p in [
            GuiParam::Lang,
            GuiParam::ScreenBrightness,
            GuiParam::AutoBrightness,
            GuiParam::Metering,
            GuiParam::BroadcastMeters,
            GuiParam::SelectedBank,
            GuiParam::PadActiveEdit,
            GuiParam::ScreenTouched,
            GuiParam::EqParamModeHigh,
        ] {
            assert_eq!(GuiParam::from_name(p.as_str()), p);
            assert!(p.is_known());
            assert_eq!(p.to_string(), p.as_str());
        }
        assert_eq!(
            GuiParam::ScreenDimAfterSeconds.as_str(),
            "screenDimAfterSeconds"
        );
        assert_eq!(
            GuiParam::InactiveButtonsBrightness.as_str(),
            "inactiveButtonsBrightness"
        );
        assert_eq!(
            GuiParam::ActiveButtonsBrightness.as_str(),
            "activeButtonsBrightness"
        );
        assert_eq!(GuiParam::EqParamModeLow.as_str(), "eqParamModeLow");
        assert_eq!(GuiParam::EqParamModeMid.as_str(), "eqParamModeMid");
    }

    #[test]
    fn gui_param_unknown_falls_back_to_other() {
        let p = GuiParam::from_name("mysteryGuiFlag");
        assert_eq!(p, GuiParam::Other("mysteryGuiFlag".to_string()));
        assert!(!p.is_known());
        assert_eq!(GuiParam::from_name(p.as_str()), p);
    }
}
