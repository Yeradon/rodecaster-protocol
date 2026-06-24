//! [`HeadphoneParam`]: the per-jack `HEADPHONE`-node parameter family.

use std::fmt;

// The two properties a real Pro II / Duo fullSync carries on each HEADPHONE
// node (confirmed from capture, firmware 1.7.3). Unlike MASTERCHANNEL / OUTPUT /
// DUCKER, HEADPHONE is multi-instance (one per physical headphone jack: 4 on the
// Pro II), so events and commands carry a headphone index.
wire_param_enum! {
    /// A per-headphone parameter: one of the flat properties the device carries
    /// on a `HEADPHONE` node. The device has one node per physical headphone jack
    /// (4 on the Pro II), so address a headphone by its index. `headphoneColour`
    /// is the jack's LED ring colour (ARGB hex string); `headphoneType` selects
    /// the connected gear profile.
    HeadphoneParam {
    Colour => "headphoneColour",
    Type => "headphoneType",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn headphone_param_round_trips_and_exact_wire_names() {
        for p in [HeadphoneParam::Colour, HeadphoneParam::Type] {
            assert_eq!(HeadphoneParam::from_name(p.as_str()), p);
            assert!(p.is_known());
            assert_eq!(p.to_string(), p.as_str());
        }
        assert_eq!(HeadphoneParam::Colour.as_str(), "headphoneColour");
        assert_eq!(HeadphoneParam::Type.as_str(), "headphoneType");
    }

    #[test]
    fn headphone_param_unknown_falls_back_to_other() {
        let p = HeadphoneParam::from_name("headphoneMystery");
        assert_eq!(p, HeadphoneParam::Other("headphoneMystery".to_string()));
        assert!(!p.is_known());
        assert_eq!(HeadphoneParam::from_name(p.as_str()), p);
    }
}
