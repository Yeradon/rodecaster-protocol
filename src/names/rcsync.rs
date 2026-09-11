//! [`RcSyncMixParam`]: per-`RCSYNCMIX`-node parameter family.
//!
//! `RCSYNCMIX` nodes model the RODECaster Sync bus's own routing matrix.
//! Six of the seven properties share wire names with the regular `MIX` cell
//! family (`mixDisabled`, `mixLevelWithAnchor`, `mixLink`, `mixLinkRequest`,
//! `mixMute`, `mixUnlinkRequest`); one is unique to rcSync
//! (`mixRcSyncLevelRequest`). Decoding distinguishes RCSYNCMIX from the
//! regular MIX matrix by path shape: regular MIX cells fall inside the
//! discovered `first_mix + source * MIX_COUNT_PER_SOURCE + mix` run, while
//! RCSYNCMIX sits elsewhere.

wire_param_enum! {
    /// A per-rcSync-mix parameter: the flat properties on each `RCSYNCMIX`
    /// node. Six wire names are shared with the regular `MIX` cell family
    /// (`mixDisabled`, `mixLevelWithAnchor`, `mixLink`, `mixLinkRequest`,
    /// `mixMute`, `mixUnlinkRequest`); `mixRcSyncLevelRequest` is
    /// rcSync-specific. The two node types are distinguished by path
    /// shape at decode time.
    RcSyncMixParam {
    MixDisabled => "mixDisabled",
    MixLevelWithAnchor => "mixLevelWithAnchor",
    MixLink => "mixLink",
    MixLinkRequest => "mixLinkRequest",
    MixMute => "mixMute",
    MixUnlinkRequest => "mixUnlinkRequest",
    MixRcSyncLevelRequest => "mixRcSyncLevelRequest",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rc_sync_mix_param_round_trips_all_seven() {
        for p in [
            RcSyncMixParam::MixDisabled,
            RcSyncMixParam::MixLevelWithAnchor,
            RcSyncMixParam::MixLink,
            RcSyncMixParam::MixLinkRequest,
            RcSyncMixParam::MixMute,
            RcSyncMixParam::MixUnlinkRequest,
            RcSyncMixParam::MixRcSyncLevelRequest,
        ] {
            assert_eq!(RcSyncMixParam::from_name(p.as_str()), p);
            assert!(p.is_known());
        }
    }
}
