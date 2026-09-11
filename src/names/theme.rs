//! [`ThemeParam`]: the singleton `THEME`-node parameter family.

// The one property the THEME node carries (confirmed from capture, firmware
// 1.7.3): the currently-selected UI theme identifier. Value is an Int
// selecting one of the device's built-in themes.
wire_param_enum! {
    /// A device-wide UI theme parameter: the one flat property the device
    /// carries on the singleton `THEME` node. Selects the current
    /// touchscreen theme (dark / light / etc: enum values are
    /// firmware-internal).
    ThemeParam {
    Id => "themeId",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn theme_param_name_round_trips() {
        assert_eq!(ThemeParam::from_name("themeId"), ThemeParam::Id);
        assert!(ThemeParam::Id.is_known());
    }
}
