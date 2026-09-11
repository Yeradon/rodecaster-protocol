//! [`BuildParam`]: the singleton `BUILD`-node parameter family.

// The five properties a real Pro II / Duo fullSync carries on the single
// BUILD node (confirmed from capture, firmware 1.7.3): the CallMe library
// version, plus GUI and mixer module git shas + version strings. All strings.
// These are read-back only (the wire doesn't accept writes on this node).
wire_param_enum! {
    /// A device-wide firmware-build metadata parameter: one of the flat
    /// properties the device carries on the singleton `BUILD` node.
    /// Read-back only (the fields are set by the firmware image itself;
    /// writes are ignored). Includes the CallMe version, and GUI + mixer
    /// module version strings and git shas: useful for capture-fidelity
    /// tooling that needs to record the exact firmware build state.
    BuildParam {
    CallMeVersion => "buildCallMeVersion",
    GuiVersion => "buildGuiVersion",
    GuiModulesGitSha => "buildGuiModulesGitSha",
    MixerVersion => "buildMixerVersion",
    MixerModulesGitSha => "buildMixerModulesGitSha",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_param_name_round_trips() {
        let params = [
            BuildParam::CallMeVersion,
            BuildParam::GuiVersion,
            BuildParam::GuiModulesGitSha,
            BuildParam::MixerVersion,
            BuildParam::MixerModulesGitSha,
        ];
        for p in params {
            assert_eq!(BuildParam::from_name(p.as_str()), p);
            assert!(p.is_known());
        }
    }
}
