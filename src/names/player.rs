//! [`PlayerParam`]: the singleton `PLAYER`-node parameter family.

use std::fmt;

// The eleven properties a real Pro II / Duo fullSync carries on the single
// PLAYER node (confirmed from capture, firmware 1.7.3): the long-form audio
// player's transport, the loaded file, and the in/out + fade envelope.
wire_param_enum! {
    /// A player parameter: one of the flat properties the device carries on the
    /// single `PLAYER` node. There is exactly one long-form player, so this
    /// family takes no addressing key. Covers transport (state, speed,
    /// position/progress), the loaded file (path, size), the jump-to-sample
    /// command, and the in/out + fade envelope (`playerEnv*`).
    PlayerParam {
    // Transport.
    State => "playerState",
    Speed => "playerSpeed",
    Progress => "playerProgress",
    CurrentPositionTime => "playerCurrentPositionTime",
    JumpToSample => "playerJumpToSample",
    // Loaded file.
    FilePath => "playerFilePath",
    FileSize => "playerFileSize",
    // In/out + fade envelope.
    EnvStart => "playerEnvStart",
    EnvStop => "playerEnvStop",
    EnvFadeIn => "playerEnvFadeIn",
    EnvFadeOut => "playerEnvFadeOut",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn player_param_name_round_trips() {
        let params = [
            PlayerParam::State,
            PlayerParam::Speed,
            PlayerParam::Progress,
            PlayerParam::CurrentPositionTime,
            PlayerParam::JumpToSample,
            PlayerParam::FilePath,
            PlayerParam::FileSize,
            PlayerParam::EnvStart,
            PlayerParam::EnvStop,
            PlayerParam::EnvFadeIn,
            PlayerParam::EnvFadeOut,
        ];
        for p in params {
            assert_eq!(PlayerParam::from_name(p.as_str()), p);
            assert!(p.is_known());
            assert_eq!(p.to_string(), p.as_str());
        }
    }

    #[test]
    fn player_param_exact_wire_names() {
        assert_eq!(PlayerParam::State.as_str(), "playerState");
        assert_eq!(PlayerParam::EnvFadeOut.as_str(), "playerEnvFadeOut");
        assert_eq!(PlayerParam::JumpToSample.as_str(), "playerJumpToSample");
    }

    #[test]
    fn player_param_unknown_falls_back_to_other() {
        let p = PlayerParam::from_name("playerMystery");
        assert_eq!(p, PlayerParam::Other("playerMystery".to_string()));
        assert!(!p.is_known());
        assert_eq!(PlayerParam::from_name(p.as_str()), p);
    }
}
