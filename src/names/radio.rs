//! Wireless-mic radio parameter families: [`RadioParam`] (pairing / lifecycle),
//! [`RadioTxParam`] (per-transmitter state), and [`RadioRxParam`]
//! (per-receiver state).

use std::fmt;

// RADIO node carries three pairing-lifecycle properties (confirmed from
// capture, firmware 1.7.3). Pair/Unpair are command channels; Paired is a
// read-back flag reporting the current pairing state.
wire_param_enum! {
    /// A wireless-radio-pairing parameter: the three flat properties on the
    /// `RADIO` node. `Pair` and `Unpair` are command channels; `Paired`
    /// reports whether a transmitter is currently paired. These are
    /// device-side entrypoints for the RODE Wireless GO integration.
    RadioParam {
    Pair => "radioPair",
    Paired => "radioPaired",
    Unpair => "radioUnpair",
    }
}

// RADIOTX nodes carry twelve per-transmitter properties (confirmed from
// capture, firmware 1.7.3). Battery / connection / signal-quality state
// (read-back) plus writable settings for gain assist, mute, pad, record.
wire_param_enum! {
    /// A per-transmitter wireless-radio parameter: the flat properties on
    /// each `RADIOTX` node. Read-back state: battery level, charging state,
    /// connection status + connection id, device serial + type, RSSI, signal
    /// quality. Writable settings: gain assist, pad, remote mute, record.
    RadioTxParam {
    BatteryLevel => "txBatteryLevel",
    ChargingState => "txChargingState",
    Connected => "txConnected",
    ConnectionId => "txConnectionId",
    DeviceSn => "txDeviceSN",
    DeviceType => "txDeviceType",
    GainAssist => "txGainAssist",
    Pad => "txPad",
    Record => "txRecord",
    RemoteMute => "txRemoteMute",
    Rssi => "txRssi",
    SignalQuality => "txSignalQuality",
    }
}

// RADIORX carries a single per-receiver property (confirmed from capture,
// firmware 1.7.3): the receiver's radio identifier.
wire_param_enum! {
    /// A per-receiver wireless-radio parameter: the one flat property on
    /// each `RADIORX` node. `RadioId` identifies which receiver this is.
    RadioRxParam {
    RadioId => "rxRadioId",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn radio_param_round_trips() {
        for p in [RadioParam::Pair, RadioParam::Paired, RadioParam::Unpair] {
            assert_eq!(RadioParam::from_name(p.as_str()), p);
            assert!(p.is_known());
        }
    }

    #[test]
    fn radio_tx_param_round_trips_all_twelve() {
        let params = [
            RadioTxParam::BatteryLevel,
            RadioTxParam::ChargingState,
            RadioTxParam::Connected,
            RadioTxParam::ConnectionId,
            RadioTxParam::DeviceSn,
            RadioTxParam::DeviceType,
            RadioTxParam::GainAssist,
            RadioTxParam::Pad,
            RadioTxParam::Record,
            RadioTxParam::RemoteMute,
            RadioTxParam::Rssi,
            RadioTxParam::SignalQuality,
        ];
        for p in params {
            assert_eq!(RadioTxParam::from_name(p.as_str()), p);
            assert!(p.is_known());
        }
    }

    #[test]
    fn radio_tx_param_preserves_capitalized_sn() {
        assert_eq!(RadioTxParam::DeviceSn.as_str(), "txDeviceSN");
    }

    #[test]
    fn radio_rx_param_round_trips() {
        assert_eq!(RadioRxParam::from_name("rxRadioId"), RadioRxParam::RadioId);
        assert!(RadioRxParam::RadioId.is_known());
    }
}
