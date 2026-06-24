//! [`MasterParam`]: the singleton `MASTERCHANNEL`-node parameter family.

use std::fmt;

// The seven properties a real Pro II / Duo fullSync carries on the single
// MASTERCHANNEL node (confirmed from capture, firmware 1.7.3): the master-bus
// Compellor (RODE's compressor/leveller) plus the master delay.
wire_param_enum! {
    /// A master-bus parameter: one of the flat properties the device carries on
    /// the single `MASTERCHANNEL` node. There is exactly one master bus, so this
    /// family takes no addressing key (unlike [`ChannelParam`], which is
    /// per-fader). Covers the master Compellor (RODE's compressor/leveller) and
    /// the master delay.
    MasterParam {
    // Compellor (master compressor / leveller).
    CompellorOn => "masterCompellorOn",
    CompellorThreshold => "masterCompellorThreshold",
    CompellorAttack => "masterCompellorAttack",
    CompellorRelease => "masterCompellorRelease",
    CompellorGain => "masterCompellorGain",
    // Master delay.
    DelayOn => "masterDelayOn",
    DelaySeconds => "masterDelaySeconds",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn master_param_name_round_trips() {
        // Every typed MASTERCHANNEL param: from_name(as_str(x)) == x.
        let params = [
            MasterParam::CompellorOn,
            MasterParam::CompellorThreshold,
            MasterParam::CompellorAttack,
            MasterParam::CompellorRelease,
            MasterParam::CompellorGain,
            MasterParam::DelayOn,
            MasterParam::DelaySeconds,
        ];
        for p in params {
            assert_eq!(MasterParam::from_name(p.as_str()), p);
            assert!(p.is_known());
            assert_eq!(p.to_string(), p.as_str());
        }
    }

    #[test]
    fn master_param_exact_wire_names() {
        // The master* prefix and Compellor casing must match the firmware.
        assert_eq!(MasterParam::CompellorOn.as_str(), "masterCompellorOn");
        assert_eq!(MasterParam::DelaySeconds.as_str(), "masterDelaySeconds");
    }

    #[test]
    fn master_param_unknown_falls_back_to_other() {
        let p = MasterParam::from_name("masterMysteryFlag");
        assert_eq!(p, MasterParam::Other("masterMysteryFlag".to_string()));
        assert!(!p.is_known());
        assert_eq!(MasterParam::from_name(p.as_str()), p);
    }
}
