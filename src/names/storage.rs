//! [`StorageVolumeParam`]: per-`STORAGEVOLUME`-node parameter family.

use std::fmt;

// The eleven properties a real Pro II / Duo fullSync carries on each
// STORAGEVOLUME node (confirmed from capture, firmware 1.7.3). One volume is
// the SD card; there may be more if other storage is attached. `storageVolume`
// nodes address by index: see `Layout::storage_volume_path`.
wire_param_enum! {
    /// A storage-volume parameter: one of the flat properties the device
    /// carries on each `STORAGEVOLUME` node. Read-back state describes the
    /// volume's capacity, free bytes, mount + format status, and the pipe-
    /// separated `storageVolumeState` progress string that the device
    /// updates live during recording (`total|used|f|f|f`). Command channel
    /// props (`Eject`, `Erase`, `Transfer`) request device-side actions.
    StorageVolumeParam {
    // Identity + capacity.
    Name => "storageVolumeName",
    Capacity => "storageVolumeCapacity",
    Free => "storageVolumeFree",
    // Mount / format lifecycle.
    Inserted => "storageVolumeInserted",
    Mounted => "storageVolumeMounted",
    Formatted => "storageVolumeFormatted",
    // Recording destination flag + live progress.
    RecDestination => "storageVolumeRecDestination",
    State => "storageVolumeState",
    // Command channel.
    Eject => "storageVolumeEject",
    Erase => "storageVolumeErase",
    Transfer => "storageVolumeTransfer",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn storage_volume_param_name_round_trips() {
        let params = [
            StorageVolumeParam::Name,
            StorageVolumeParam::Capacity,
            StorageVolumeParam::Free,
            StorageVolumeParam::Inserted,
            StorageVolumeParam::Mounted,
            StorageVolumeParam::Formatted,
            StorageVolumeParam::RecDestination,
            StorageVolumeParam::State,
            StorageVolumeParam::Eject,
            StorageVolumeParam::Erase,
            StorageVolumeParam::Transfer,
        ];
        for p in params {
            assert_eq!(StorageVolumeParam::from_name(p.as_str()), p);
            assert!(p.is_known());
        }
    }

    #[test]
    fn storage_volume_param_state_pipe_string_is_caller_owned() {
        // `storageVolumeState` values arrive as pipe-separated strings like
        // "15927934976|11134074880|1|1|1": the crate types the property name,
        // parsing the fields is the caller's job.
        assert_eq!(StorageVolumeParam::State.as_str(), "storageVolumeState");
    }
}
