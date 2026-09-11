//! Show-system parameter families: [`ShowParam`] (per-show), [`CurrentShowParam`]
//! (the loaded show), and [`ShowControlParam`] (the show-lifecycle command
//! channel).

// The seven properties each SHOW node carries under the SHOWS container
// (confirmed from capture, firmware 1.7.3): identity (name / UID / UUID),
// storage location, snapshot index, an icon, and a modification timestamp.
wire_param_enum! {
    /// A per-show parameter: one of the flat properties the device carries on
    /// each `SHOW` child under the singleton `SHOWS` container. `Uid` is a
    /// stable integer; `Uuid` is a canonical string identifier;
    /// `SnapshotIndex` selects which snapshot of the show is active;
    /// `LastModification` is a wall-clock timestamp string.
    ShowParam {
    Icon => "showIcon",
    LastModification => "showLastModification",
    Name => "showName",
    SnapshotIndex => "showSnapshotIndex",
    StorageType => "showStorageType",
    Uid => "showUID",
    Uuid => "showUUID",
    }
}

// The three properties the singleton CURRENTSHOW node carries (confirmed from
// capture, firmware 1.7.3): the identity of the show that's currently loaded.
wire_param_enum! {
    /// A currently-loaded-show parameter: one of the flat properties the
    /// device carries on the singleton `CURRENTSHOW` node. Reflects which
    /// [`ShowParam`]-addressed show is currently active on the device.
    CurrentShowParam {
    Icon => "currentShowIcon",
    Name => "currentShowName",
    Uuid => "currentShowUUID",
    }
}

// The ten properties the singleton SHOWCONTROL node carries (confirmed from
// capture, firmware 1.7.3): the write-channel for show lifecycle actions
// (delete / export / import / new-from-default) plus read-back progress and
// the last-error string.
wire_param_enum! {
    /// A show-lifecycle-command parameter: one of the flat properties the
    /// device carries on the singleton `SHOWCONTROL` node. This is the
    /// write channel for creating, deleting, exporting, and importing
    /// shows (`showControlDelete`, `showControlExport`, `showControlImport`,
    /// `showControlNewFromDefault*`), plus the read-back `showControlProgress`
    /// and `showControlLastError` for observing an in-progress action.
    ShowControlParam {
    Delete => "showControlDelete",
    Export => "showControlExport",
    ExportImport => "showControlExportImport",
    Import => "showControlImport",
    LastError => "showControlLastError",
    LockoutCentral => "showControlLockoutCentral",
    NewFromDefault => "showControlNewFromDefault",
    NewFromDefaultMuted => "showControlNewFromDefaultMuted",
    Progress => "showControlProgress",
    Updating => "showControlUpdating",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn show_param_name_round_trips() {
        let params = [
            ShowParam::Icon,
            ShowParam::LastModification,
            ShowParam::Name,
            ShowParam::SnapshotIndex,
            ShowParam::StorageType,
            ShowParam::Uid,
            ShowParam::Uuid,
        ];
        for p in params {
            assert_eq!(ShowParam::from_name(p.as_str()), p);
            assert!(p.is_known());
        }
        assert_eq!(ShowParam::Uid.as_str(), "showUID");
        assert_eq!(ShowParam::Uuid.as_str(), "showUUID");
    }

    #[test]
    fn current_show_param_name_round_trips() {
        for p in [
            CurrentShowParam::Icon,
            CurrentShowParam::Name,
            CurrentShowParam::Uuid,
        ] {
            assert_eq!(CurrentShowParam::from_name(p.as_str()), p);
            assert!(p.is_known());
        }
    }

    #[test]
    fn show_control_param_name_round_trips_all_ten() {
        let params = [
            ShowControlParam::Delete,
            ShowControlParam::Export,
            ShowControlParam::ExportImport,
            ShowControlParam::Import,
            ShowControlParam::LastError,
            ShowControlParam::LockoutCentral,
            ShowControlParam::NewFromDefault,
            ShowControlParam::NewFromDefaultMuted,
            ShowControlParam::Progress,
            ShowControlParam::Updating,
        ];
        for p in params {
            assert_eq!(ShowControlParam::from_name(p.as_str()), p);
            assert!(p.is_known());
        }
    }
}
