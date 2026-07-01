//! StreamerX-integration parameter families: [`StreamerXMixPresetParam`]
//! (per-preset) and [`StreamerXStreamMixParam`] (per-stream).

use std::fmt;

// STREAMERXMIXPRESET nodes carry two properties per preset (confirmed from
// capture, firmware 1.7.3): a creation-status flag and the preset name.
wire_param_enum! {
    /// A per-preset StreamerX mix preset parameter: the flat properties on
    /// each `STREAMERXMIXPRESET` node. `Created` reports whether the preset
    /// has been initialized; `Name` is the user-facing preset name.
    StreamerXMixPresetParam {
    Created => "streamerXPresetCreated",
    Name => "streamerXPresetName",
    }
}

// STREAMERXSTREAMMIX carries a single mix-level property (confirmed 1.7.3).
// Firmware wire name has a duplicated "mix" prefix (`streammixmixLevel`),
// preserved verbatim.
wire_param_enum! {
    /// A per-stream StreamerX mix parameter: the one flat property on each
    /// `STREAMERXSTREAMMIX` node. Firmware wire name has a duplicated `mix`
    /// prefix (`streammixmixLevel`); preserved verbatim.
    StreamerXStreamMixParam {
    MixLevel => "streammixmixLevel",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn streamerx_mix_preset_param_round_trips() {
        for p in [
            StreamerXMixPresetParam::Created,
            StreamerXMixPresetParam::Name,
        ] {
            assert_eq!(StreamerXMixPresetParam::from_name(p.as_str()), p);
            assert!(p.is_known());
        }
    }

    #[test]
    fn streamerx_stream_mix_param_preserves_duplicated_mix_prefix() {
        assert_eq!(
            StreamerXStreamMixParam::MixLevel.as_str(),
            "streammixmixLevel"
        );
    }
}
