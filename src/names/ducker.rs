//! [`DuckerParam`]: the singleton `DUCKER`-node parameter family.

// The single property a real Pro II / Duo fullSync carries on the single DUCKER
// node (confirmed from capture, firmware 1.7.3): the auto-duck depth in dB.
wire_param_enum! {
    /// A ducker parameter: the flat property the device carries on the single
    /// `DUCKER` node. There is exactly one ducker, so this family takes no
    /// addressing key. Auto-ducking lowers other faders when a mic-flagged
    /// source is active; `duckerDepth` is how far (in dB) it pulls them down.
    DuckerParam {
    Depth => "duckerDepth",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ducker_param_round_trips_and_exact_wire_name() {
        assert_eq!(DuckerParam::from_name("duckerDepth"), DuckerParam::Depth);
        assert_eq!(DuckerParam::Depth.as_str(), "duckerDepth");
        assert!(DuckerParam::Depth.is_known());
        assert_eq!(DuckerParam::Depth.to_string(), "duckerDepth");
    }

    #[test]
    fn ducker_param_unknown_falls_back_to_other() {
        let p = DuckerParam::from_name("duckerMystery");
        assert_eq!(p, DuckerParam::Other("duckerMystery".to_string()));
        assert!(!p.is_known());
        assert_eq!(DuckerParam::from_name(p.as_str()), p);
    }
}
