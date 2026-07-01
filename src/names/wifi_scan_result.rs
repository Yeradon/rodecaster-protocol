//! [`WifiScanResultParam`]: per-`WIFISCANRESULT`-node parameter family.

use std::fmt;

// WIFISCANRESULT nodes carry a single property per scan result (confirmed
// from capture, firmware 1.7.3): the SSID string. The device populates a
// contiguous run of these nodes as WiFi scans complete; each holds one
// discovered network's SSID (plus, empirically, encoding hints appended
// with `----` separators, e.g. `"WLAN1000----PSK"`).
wire_param_enum! {
    /// A per-WiFi-scan-result parameter: the one flat property on each
    /// `WIFISCANRESULT` node. The device populates these as WiFi scans
    /// complete. Value is a String encoded as `SSID----ENCODING` (the
    /// device appends the encryption type with a `----` separator).
    WifiScanResultParam {
    Ssid => "wifiScanResultSSID",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wifi_scan_result_param_round_trips() {
        assert_eq!(
            WifiScanResultParam::from_name("wifiScanResultSSID"),
            WifiScanResultParam::Ssid
        );
        assert!(WifiScanResultParam::Ssid.is_known());
        assert_eq!(WifiScanResultParam::Ssid.as_str(), "wifiScanResultSSID");
    }
}
