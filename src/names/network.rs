//! [`NetworkParam`]: the singleton `NETWORK`-node parameter family.

use std::fmt;

// The fifty-six properties a real Pro II / Duo fullSync carries on the single
// NETWORK node (confirmed from capture, firmware 1.7.3): Bluetooth pairing +
// scan slots, cellular link, wired networking, WiFi PSK / SSID / scan slots.
// The `bt*Scan1..10` and `wifiScan1..10` slots are the discovery-results
// buffers the device fills as it scans; `btPairedNumber1..5` remembers the
// last five paired devices.
wire_param_enum! {
    /// A device-wide networking parameter: one of the flat properties the
    /// device carries on the singleton `NETWORK` node. Covers Bluetooth
    /// (visibility, scan trigger, pairing, connection state, ten scan
    /// result slots, five paired-device memory slots), cellular data
    /// (APN, IP config, USB modem presence), wired IP (gateway, DNS,
    /// static-IP toggle), WiFi (SSID, PSK, IP config, ten scan result
    /// slots, DHCP), and the wired-Ethernet connected flag. Un-typed
    /// properties fall back to `NetworkParam::Other` for forward-compat.
    NetworkParam {
    // Bluetooth visibility + scan trigger.
    BtVisible => "btVisible",
    BtDoScan => "btDoScan",
    // Bluetooth pairing / connection lifecycle.
    BtDoPair => "btDoPair",
    BtDoUnPair => "btDoUnPair",
    BtDoConnect => "btDoConnect",
    BtDoDisconnect => "btDoDisconnect",
    BtPairCode => "btPairCode",
    BtConnectedAddress => "btConnectedAddress",
    BtConnectedType => "btConnectedType",
    // Bluetooth paired-device memory (last five).
    BtPairedNumber1 => "btPairedNumber1",
    BtPairedNumber2 => "btPairedNumber2",
    BtPairedNumber3 => "btPairedNumber3",
    BtPairedNumber4 => "btPairedNumber4",
    BtPairedNumber5 => "btPairedNumber5",
    // Bluetooth scan-result slots (ten).
    BtScan1 => "btScan1",
    BtScan2 => "btScan2",
    BtScan3 => "btScan3",
    BtScan4 => "btScan4",
    BtScan5 => "btScan5",
    BtScan6 => "btScan6",
    BtScan7 => "btScan7",
    BtScan8 => "btScan8",
    BtScan9 => "btScan9",
    BtScan10 => "btScan10",
    // Cellular data link.
    CellApn => "cellAPN",
    CellEnabled => "cellEnabled",
    CellGateway => "cellGateway",
    CellIpAddress => "cellIpAddress",
    CellSubnetMask => "cellSubnetMask",
    CellUsbFound => "cellUSBFound",
    // Wired IP configuration.
    Gateway => "gateway",
    IpAddress => "ipAddress",
    PrimaryDns => "primaryDns",
    SecondaryDns => "secondaryDns",
    StaticIpSet => "staticIpSet",
    SubnetMask => "subnetMask",
    // Wired Ethernet link state.
    WiredConnected => "wiredConnected",
    // WiFi state + credentials.
    Wifi => "wifi",
    WifiDhcp => "wifiDHCP",
    WifiGateway => "wifiGateway",
    WifiIncorrectPsk => "wifiIncorrectPSK",
    WifiIpAddress => "wifiIpAddress",
    WifiPsk => "wifiPSK",
    WifiSsid => "wifiSSID",
    WifiSubnetMask => "wifiSubnetMask",
    // WiFi scan trigger.
    WifiScan => "wifiScan",
    // WiFi scan-result slots (ten).
    WifiScan1 => "wifiScan1",
    WifiScan2 => "wifiScan2",
    WifiScan3 => "wifiScan3",
    WifiScan4 => "wifiScan4",
    WifiScan5 => "wifiScan5",
    WifiScan6 => "wifiScan6",
    WifiScan7 => "wifiScan7",
    WifiScan8 => "wifiScan8",
    WifiScan9 => "wifiScan9",
    WifiScan10 => "wifiScan10",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn network_param_name_round_trips_all() {
        let params = [
            NetworkParam::BtVisible,
            NetworkParam::BtDoScan,
            NetworkParam::BtConnectedAddress,
            NetworkParam::CellApn,
            NetworkParam::Gateway,
            NetworkParam::Wifi,
            NetworkParam::WifiScan1,
            NetworkParam::WiredConnected,
        ];
        for p in params {
            assert_eq!(NetworkParam::from_name(p.as_str()), p);
            assert!(p.is_known());
        }
    }

    #[test]
    fn network_param_preserves_firmware_casing() {
        // The wire names carry firmware casing quirks (APN, DHCP, PSK, USB, SSID).
        assert_eq!(NetworkParam::CellApn.as_str(), "cellAPN");
        assert_eq!(NetworkParam::WifiDhcp.as_str(), "wifiDHCP");
        assert_eq!(NetworkParam::WifiPsk.as_str(), "wifiPSK");
        assert_eq!(NetworkParam::WifiIncorrectPsk.as_str(), "wifiIncorrectPSK");
        assert_eq!(NetworkParam::WifiSsid.as_str(), "wifiSSID");
        assert_eq!(NetworkParam::CellUsbFound.as_str(), "cellUSBFound");
    }

    #[test]
    fn network_param_unknown_falls_back_to_other() {
        let p = NetworkParam::from_name("wifiSomethingElse");
        assert_eq!(p, NetworkParam::Other("wifiSomethingElse".to_string()));
        assert!(!p.is_known());
    }
}
