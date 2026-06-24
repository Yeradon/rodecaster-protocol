//! [`SystemParam`]: the singleton root `SYSTEM`-node device-state family.

use std::fmt;

// The forty-nine properties a real Pro II / Duo fullSync carries on the single
// root-level SYSTEM node (confirmed from capture, firmware 1.7.3). SYSTEM is the
// device-wide state singleton: identity (serial, firmware, board type), the
// firmware/app update + download lifecycle, the date/time + personalization
// settings, the global output disables, and USB / storage / sharing status. None
// of it is per-strip audio routing.
//
// Note the `system` prefix is NOT uniform: the identity/date/personalization
// props carry it (`systemFirmwareVersion`, `systemDateTimezone`, ...), but the
// update lifecycle, the disables, and the USB/storage status props do not
// (`updateChecking`, `disableAllLineoutOutputs`, `usbHostOnUsb2`, ...). The
// variant names strip the `system` prefix where present and otherwise PascalCase
// the wire name verbatim, so each `=> "wireName"` is the exact firmware string.
wire_param_enum! {
    /// A device-wide system parameter: one of the flat properties the device
    /// carries on the single root `SYSTEM` node. This is global device state, not
    /// per-strip audio routing: identity (serial / firmware / board type), the
    /// firmware-update + download lifecycle, date/time + personalization
    /// settings, the global output disables, and USB / storage / sharing status.
    ///
    /// Two things worth calling out:
    /// - [`SystemParam::PowerOffRequest`] (`powerOffRequest`) is the *readback /
    ///   generic-write* identity of the power-off flag. It is distinct from
    ///   [`crate::Command::PowerOff`], which is a dedicated, layout-independent
    ///   power-off frame pinned to a fixed node index. Writing this param via
    ///   [`crate::Command::SetSystemParam`] instead addresses the discovered
    ///   `SYSTEM` node; on firmware 1.7.3 that resolves to the same node, so the
    ///   two are equivalent there.
    /// - Several wire names carry firmware casing/spelling quirks kept verbatim:
    ///   `updateViaUSB`, `lastRecordingID`, `updateResetAfterFWURequested`, and
    ///   the misspelled `usbHostUnspportedFirmware`.
    SystemParam {
    // Identity / selection / engine.
    MidiControl => "systemMidiControl",
    ChannelSelected => "systemChannelSelected",
    MixSelected => "systemMixSelected",
    FirmwareVersion => "systemFirmwareVersion",
    EngineMode => "engineMode",
    SerialNumber => "systemSerialNumber",
    BoardType => "boardType",
    // Date / time + clock display.
    DateTimezone => "systemDateTimezone",
    DateTimeDaylightSavings => "systemDateTimeDaylightSavings",
    DateTimeOnHome => "systemDateTimeOnHome",
    DateTime24h => "systemDateTime24h",
    // Device behaviour / personalization.
    BetaMode => "systemBetaMode",
    HapticSetting => "systemHapticSetting",
    RecButtonSetting => "systemRecButtonSetting",
    UnifyMode => "unifyMode",
    PresenterMode => "presenterMode",
    PresenterSoftware => "presenterSoftware",
    AssignableMeterSource => "assignableMeterSource",
    LastRecordingId => "lastRecordingID",
    TransferModeType => "transferModeType",
    // Firmware / app update lifecycle (status flags).
    AppUpdateAvailable => "appUpdateAvailable",
    OsUpdateAvailable => "osUpdateAvailable",
    UpdateDownloaded => "updateDownloaded",
    UpdateChecking => "updateChecking",
    UpdateNoInternet => "updateNoInternet",
    UpdateComplete => "updateComplete",
    UpdateDownloadProgress => "updateDownloadProgress",
    UpdateInstalledProgress => "updateInstalledProgress",
    UpdateVersion => "updateVersion",
    UpdateViaUsb => "updateViaUSB",
    // Update / download command channel (request flags).
    UpdateCheckRequested => "updateCheckRequested",
    UpdateInitiateRequested => "updateInitiateRequested",
    DownloadInitiateRequested => "downloadInitiateRequested",
    DownloadCancelRequested => "downloadCancelRequested",
    UpdateRebootRequested => "updateRebootRequested",
    UpdateResetDeviceRequested => "updateResetDeviceRequested",
    UpdateResetAppRequested => "updateResetAppRequested",
    UpdateResetAfterFwuRequested => "updateResetAfterFWURequested",
    // Power.
    PowerOffRequest => "powerOffRequest",
    // Global output disables.
    DisableAllHeadphoneOutputs => "disableAllHeadphoneOutputs",
    DisableAllLineoutOutputs => "disableAllLineoutOutputs",
    DisableAllPhysicalButtons => "disableAllPhysicalButtons",
    // USB / storage / hardware status.
    Usb1Connected => "usb1Connected",
    UsbHostOnUsb2 => "usbHostOnUsb2",
    UsbHostUnsupportedFirmware => "usbHostUnspportedFirmware",
    RemountPadStorage => "remountPadStorage",
    BadPadDetected => "badPadDetected",
    // Sharing / analytics.
    ShareAnom => "shareAnom",
    ShareResource => "shareResource",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn system_param_round_trips_and_exact_wire_names() {
        for p in [
            SystemParam::MidiControl,
            SystemParam::FirmwareVersion,
            SystemParam::BoardType,
            SystemParam::EngineMode,
            SystemParam::DateTimezone,
            SystemParam::BetaMode,
            SystemParam::PowerOffRequest,
            SystemParam::DisableAllLineoutOutputs,
            SystemParam::UsbHostOnUsb2,
            SystemParam::ShareAnom,
        ] {
            assert_eq!(SystemParam::from_name(p.as_str()), p);
            assert!(p.is_known());
            assert_eq!(p.to_string(), p.as_str());
        }
        // The `system`-prefixed identity/date props strip the prefix.
        assert_eq!(SystemParam::SerialNumber.as_str(), "systemSerialNumber");
        assert_eq!(
            SystemParam::ChannelSelected.as_str(),
            "systemChannelSelected"
        );
        assert_eq!(SystemParam::DateTime24h.as_str(), "systemDateTime24h");
        // Firmware casing/spelling quirks must survive verbatim.
        assert_eq!(SystemParam::UpdateViaUsb.as_str(), "updateViaUSB");
        assert_eq!(SystemParam::LastRecordingId.as_str(), "lastRecordingID");
        assert_eq!(
            SystemParam::UpdateResetAfterFwuRequested.as_str(),
            "updateResetAfterFWURequested"
        );
        assert_eq!(
            SystemParam::UsbHostUnsupportedFirmware.as_str(),
            "usbHostUnspportedFirmware"
        );
    }

    #[test]
    fn system_param_unknown_falls_back_to_other() {
        let p = SystemParam::from_name("mysterySystemFlag");
        assert_eq!(p, SystemParam::Other("mysterySystemFlag".to_string()));
        assert!(!p.is_known());
        assert_eq!(SystemParam::from_name(p.as_str()), p);
    }
}
