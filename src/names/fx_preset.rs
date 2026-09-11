//! [`FxPresetParam`]: per-`FXPRESET`-node parameter family.

// FXPRESET nodes carry two properties per preset (confirmed from capture,
// firmware 1.7.3): the effect preset's serialized contents blob and its
// preset index.
wire_param_enum! {
    /// A per-effect-preset parameter: the flat properties on each `FXPRESET`
    /// node. `Contents` is the preset's serialized data (typically a
    /// pipe-separated or JSON-ish blob); `Idx` is the preset slot ordinal.
    FxPresetParam {
    Contents => "fxPresetContents",
    Idx => "fxPresetIdx",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fx_preset_param_round_trips() {
        for p in [FxPresetParam::Contents, FxPresetParam::Idx] {
            assert_eq!(FxPresetParam::from_name(p.as_str()), p);
            assert!(p.is_known());
        }
    }
}
