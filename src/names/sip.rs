//! SIP subsystem parameter families: [`SipCallingParam`],
//! [`SipRegistrationParam`], [`SipCallSlotsParam`], [`SipAdvancedParam`].
//!
//! Empirically measured on Duo fw 1.7.3 (2026-07-01) via CLI raw-write fuzzing:
//! every SIPADVANCED property accepts writes and persists them across
//! `refresh`. Registration-relevant writes trigger a
//! `sipRegistrationIsRegistered` echo as a device-side re-check. Statistical
//! properties on SIPCALLSLOTS (`sipSlotCallQuality`, `sipSlotCallJitter`,
//! `sipSlotCallBitrate`, `sipSlotCallPacketLoss`) are device-managed
//! read-back only.

use std::fmt;

// SIPCALLING node (fourteen properties, plus SIPREGISTRATION sub-nodes).
// Singleton at root; the device presents itself via `sipRodeCode`, tracks
// remaining calltime, and hosts + accepts + emits call setup metadata here.
wire_param_enum! {
    /// A SIP calling-level parameter: one of the fourteen flat properties the
    /// device carries on the singleton `SIPCALLING` node. Combines settings
    /// (`HostingEnabled` / `HostingToggleEnabled` / `LicenceMode` /
    /// `LicenceCheck`), device-managed read-back (`RodeCode` — the invite
    /// code; rotates on hosting toggle), incoming/outgoing call setup
    /// channels (JSON-ish strings), the pending-slot selector, subscription
    /// remaining-time meters, and the post-call rating write channel.
    SipCallingParam {
    HostingEnabled => "sipCallHostingEnabled",
    HostingToggleEnabled => "sipCallHostingToggleEnabled",
    LicenceMode => "sipLicenceMode",
    LicenceCheck => "sipLicenceCheck",
    RodeCode => "sipRodeCode",
    IncomingCallAccept => "sipIncomingCallAccept",
    IncomingCallDetails => "sipIncomingCallDetails",
    OutgoingCallDetails => "sipOutgoingCallDetails",
    SlotPendingCallSetup => "sipSlotPendingCallSetup",
    RemainingCalltime => "sipRemainingCalltime",
    RemainingWebCalltime => "sipRemainingWebCalltime",
    RenewalDate => "sipRenewalDate",
    LicenceRenewal => "sipLicenceRenewal",
    CallRating => "sipCallRating",
    }
}

// SIPREGISTRATION node (three properties per registration). Per-instance under
// SIPCALLING — the Duo has two registration slots preconfigured. `Details`
// carries the account registration payload; `IsRegistered` is a device-side
// read-back that echoes on every registration re-check.
wire_param_enum! {
    /// A per-registration SIP parameter: one of the three flat properties the
    /// device carries on each `SIPREGISTRATION` child under `SIPCALLING`.
    /// `Index` selects which registration slot; `Details` is the account
    /// registration payload; `IsRegistered` is the device-side registered
    /// flag (read-back — echoes whenever the SIP subsystem re-checks
    /// registration, including as a side effect of writes to SIPADVANCED
    /// account fields).
    SipRegistrationParam {
    Index => "sipRegistrationIndex",
    Details => "sipRegistrationDetails",
    IsRegistered => "sipRegistrationIsRegistered",
    }
}

// SIPCALLSLOTS nodes (twelve properties per slot). Per-slot at root; the Duo
// has three consecutive call slots. Read-back statistics (`Quality`, `Jitter`,
// `Bitrate`, `PacketLoss`) are device-managed; command channels
// (`Disconnect`, `Extend`) accept writes; identity fields (`SlotId`,
// `State`, `UUID`, `Address`, `IsHost`, `Mode`) mostly read-back.
wire_param_enum! {
    /// A per-slot SIP call parameter: one of the twelve flat properties the
    /// device carries on each `SIPCALLSLOTS` node (three consecutive slots
    /// at root). Statistics (`Quality`, `Jitter`, `Bitrate`, `PacketLoss`)
    /// are device-managed. `Disconnect` is a `Binary` command channel
    /// (same request-pulse pattern as `mixLinkRequest`); `Extend` is a
    /// bool command. Identity/state fields (`SlotId`, `State`, `UUID`,
    /// `Address`, `IsHost`, `Mode`) reflect the call's current status.
    SipCallSlotsParam {
    SlotId => "sipCallSlotId",
    State => "sipSlotCallState",
    Uuid => "sipSlotCallUUID",
    Address => "sipSlotCallAddress",
    Quality => "sipSlotCallQuality",
    Jitter => "sipSlotCallJitter",
    Bitrate => "sipSlotCallBitrate",
    PacketLoss => "sipSlotCallPacketLoss",
    Disconnect => "sipSlotCallDisconnect",
    Extend => "sipSlotCallExtend",
    IsHost => "sipSlotCallIsHost",
    Mode => "sipSlotCallMode",
    }
}

// SIPADVANCED node (twenty-five properties). Singleton at root — every one of
// these was proven writable + persistent in the 2026-07-01 fuzz session.
// Notably the device stores arbitrary property names verbatim on this node
// (no schema enforcement); the typed set here is the observed-25.
wire_param_enum! {
    /// A SIP advanced-settings parameter: one of the twenty-five flat
    /// properties the device carries on the singleton `SIPADVANCED` node.
    /// Every property in this family is empirically writable + persistent
    /// on Duo fw 1.7.3 (measured 2026-07-01). Groups: codec / feature
    /// flags (`ForceCodecChoice`, `EnableVideoStream`, `EnableRFCDuplication`,
    /// `IncomingNameFilter`, `UsbNumberPad`, `NonHTTPSInterface`,
    /// `AutoReconnect`); audio routing + DTMF (`IncomingAudioRouting`,
    /// `DtmfMode`); receive jitter buffer bounds; the sipAccount* config
    /// (username / password / domain / proxy / transport / auth), a NAT
    /// traversal block (`sipNATTraversal*`), the unit name, and the
    /// device's rodeCallQuality read-back.
    ///
    /// Writes to registration-relevant fields (account credentials, NAT,
    /// domain) trigger a `sipRegistrationIsRegistered` echo as the device
    /// re-checks its registration state.
    SipAdvancedParam {
    ForceCodecChoice => "forceCodecChoice",
    EnableVideoStream => "enableVideoStream",
    EnableRFCDuplication => "enableRFCDuplication",
    IncomingNameFilter => "incomingNameFilter",
    UsbNumberPad => "usbNumberPad",
    NonHTTPSInterface => "nonHTTPSInterface",
    AutoReconnect => "autoReconnect",
    IncomingAudioRouting => "incomingAudioRouting",
    DtmfMode => "dtmfMode",
    ReceiveJitterBufferMin => "receiveJitterBufferMin",
    ReceiveJitterBufferMax => "receiveJitterBufferMax",
    SipWebPassword => "sipWebPassword",
    SipAccountRegister => "sipAccountRegister",
    SipAccountUsername => "sipAccountUsername",
    SipAccountPassword => "sipAccountPassword",
    SipAccountDomain => "sipAccountDomain",
    SipAccountProxyAddress => "sipAccountProxyAddress",
    SipAccountTransport => "sipAccountTransport",
    SipNatTraversalMode => "sipNATTraversalMode",
    SipNatTraversalServer => "sipNATTraversalServer",
    SipNatTraversalUsername => "sipNATTraversalUsername",
    SipNatTraversalPassword => "sipNATTraversalPassword",
    SipUnitName => "sipUnitName",
    SipAccountAuthUsername => "sipAccountAuthUsername",
    RodeCallQuality => "rodeCallQuality",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sip_calling_param_round_trips() {
        let params = [
            SipCallingParam::HostingEnabled,
            SipCallingParam::HostingToggleEnabled,
            SipCallingParam::LicenceMode,
            SipCallingParam::LicenceCheck,
            SipCallingParam::RodeCode,
            SipCallingParam::IncomingCallAccept,
            SipCallingParam::IncomingCallDetails,
            SipCallingParam::OutgoingCallDetails,
            SipCallingParam::SlotPendingCallSetup,
            SipCallingParam::RemainingCalltime,
            SipCallingParam::RemainingWebCalltime,
            SipCallingParam::RenewalDate,
            SipCallingParam::LicenceRenewal,
            SipCallingParam::CallRating,
        ];
        for p in params {
            assert_eq!(SipCallingParam::from_name(p.as_str()), p);
            assert!(p.is_known());
        }
    }

    #[test]
    fn sip_registration_param_round_trips() {
        for p in [
            SipRegistrationParam::Index,
            SipRegistrationParam::Details,
            SipRegistrationParam::IsRegistered,
        ] {
            assert_eq!(SipRegistrationParam::from_name(p.as_str()), p);
            assert!(p.is_known());
        }
    }

    #[test]
    fn sip_call_slots_param_round_trips_all_twelve() {
        let params = [
            SipCallSlotsParam::SlotId,
            SipCallSlotsParam::State,
            SipCallSlotsParam::Uuid,
            SipCallSlotsParam::Address,
            SipCallSlotsParam::Quality,
            SipCallSlotsParam::Jitter,
            SipCallSlotsParam::Bitrate,
            SipCallSlotsParam::PacketLoss,
            SipCallSlotsParam::Disconnect,
            SipCallSlotsParam::Extend,
            SipCallSlotsParam::IsHost,
            SipCallSlotsParam::Mode,
        ];
        for p in params {
            assert_eq!(SipCallSlotsParam::from_name(p.as_str()), p);
            assert!(p.is_known());
        }
    }

    #[test]
    fn sip_advanced_param_round_trips_all_twenty_five() {
        let params = [
            SipAdvancedParam::ForceCodecChoice,
            SipAdvancedParam::EnableVideoStream,
            SipAdvancedParam::EnableRFCDuplication,
            SipAdvancedParam::IncomingNameFilter,
            SipAdvancedParam::UsbNumberPad,
            SipAdvancedParam::NonHTTPSInterface,
            SipAdvancedParam::AutoReconnect,
            SipAdvancedParam::IncomingAudioRouting,
            SipAdvancedParam::DtmfMode,
            SipAdvancedParam::ReceiveJitterBufferMin,
            SipAdvancedParam::ReceiveJitterBufferMax,
            SipAdvancedParam::SipWebPassword,
            SipAdvancedParam::SipAccountRegister,
            SipAdvancedParam::SipAccountUsername,
            SipAdvancedParam::SipAccountPassword,
            SipAdvancedParam::SipAccountDomain,
            SipAdvancedParam::SipAccountProxyAddress,
            SipAdvancedParam::SipAccountTransport,
            SipAdvancedParam::SipNatTraversalMode,
            SipAdvancedParam::SipNatTraversalServer,
            SipAdvancedParam::SipNatTraversalUsername,
            SipAdvancedParam::SipNatTraversalPassword,
            SipAdvancedParam::SipUnitName,
            SipAdvancedParam::SipAccountAuthUsername,
            SipAdvancedParam::RodeCallQuality,
        ];
        for p in params {
            assert_eq!(SipAdvancedParam::from_name(p.as_str()), p);
            assert!(p.is_known());
        }
    }

    #[test]
    fn sip_advanced_param_preserves_firmware_casing() {
        // Firmware uses HTTPS / RFC / NAT / HTTPS in wire names; preserved verbatim.
        assert_eq!(
            SipAdvancedParam::EnableRFCDuplication.as_str(),
            "enableRFCDuplication"
        );
        assert_eq!(
            SipAdvancedParam::NonHTTPSInterface.as_str(),
            "nonHTTPSInterface"
        );
        assert_eq!(
            SipAdvancedParam::SipNatTraversalMode.as_str(),
            "sipNATTraversalMode"
        );
    }
}
