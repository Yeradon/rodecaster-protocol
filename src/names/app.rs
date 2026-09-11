//! [`AppParam`]: the singleton `APP`-node parameter family.

use std::fmt;

// The four properties a real Pro II / Duo fullSync carries on the single APP
// node (confirmed from capture, firmware 1.7.3): companion-app-mode flags.
// These reflect the device's understanding of the RODE Connect app's current
// state: used to auto-configure DSP for common companion-app workflows.
wire_param_enum! {
    /// A companion-app-mode parameter: one of the flat properties the device
    /// carries on the singleton `APP` node. Reflects the device's view of
    /// the RODE Connect app's current mode (compression on, monitor mix
    /// selection, output device chosen, recording state). Semantics of each
    /// value are firmware-internal; the crate types the property name only.
    AppParam {
    Compression => "appCompression",
    MonitorMix => "appMonitorMix",
    OutputDevice => "appOutputDevice",
    Recording => "appRecording",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_param_name_round_trips() {
        let params = [
            AppParam::Compression,
            AppParam::MonitorMix,
            AppParam::OutputDevice,
            AppParam::Recording,
        ];
        for p in params {
            assert_eq!(AppParam::from_name(p.as_str()), p);
            assert!(p.is_known());
        }
    }
}
