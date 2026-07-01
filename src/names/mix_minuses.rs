//! [`MixMinusesParam`]: per-`MIXMINUSES`/`RCSYNCMIXMINUES`-node parameter
//! family.
//!
//! Both `MIXMINUSES` and `RCSYNCMIXMINUES` nodes carry the SAME single
//! property (`outputMixMinus`). Consumers who need to distinguish between the
//! two node types must inspect the path — this family types the property name
//! only. The firmware misspells `RCSYNCMIXMINUES` (should be `MINUSES`);
//! preserved verbatim in the node-name check.

use std::fmt;

wire_param_enum! {
    /// A mix-minus parameter: the one flat property on both `MIXMINUSES` and
    /// `RCSYNCMIXMINUES` nodes. Mix-minus routing produces a per-output
    /// send with the local channel subtracted (so a caller doesn't hear their
    /// own voice returning via CallMe / SIP / rcSync mixes).
    MixMinusesParam {
    OutputMixMinus => "outputMixMinus",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mix_minuses_param_round_trips() {
        assert_eq!(
            MixMinusesParam::from_name("outputMixMinus"),
            MixMinusesParam::OutputMixMinus
        );
        assert!(MixMinusesParam::OutputMixMinus.is_known());
    }
}
