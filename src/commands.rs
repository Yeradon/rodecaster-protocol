//! Typed Rodecaster commands.
//!
//! `Command` is the outgoing vocabulary the server (or any consumer) builds.
//! [`Command::encode`] produces one or more JUCE change-frame payloads,
//! addressed through a [`crate::Layout`] discovered from the device's
//! fullSync. The encoder routes every command through
//! [`crate::change_frame::encode_property_changed`], so the wire format is
//! always JUCE-faithful and the ID math lives in one place (the `Layout`),
//! never duplicated across files.
//!
//! Wrap each returned payload in a [`crate::frame::Packet`] for the transport.

use crate::change_frame;
use crate::juce_var::Value;
use crate::layout::Layout;

/// Binary "request" blob the device expects for `mixLinkRequest` /
/// `mixUnlinkRequest`. Verified verbatim against the server's existing
/// encoders + the device's parser.
const MIX_LINK_REQUEST_BLOB: [u8; 6] = [0x01, 0x01, 0x02, 0x01, 0x01, 0x02];

/// JUCE `Int` value used to mean "unassigned" for `channelInputSource`.
/// The device treats negative source ids as "no source"; on the wire JUCE's
/// `INT` marker is a fixed 4-byte little-endian `i32`, so `-1` serializes as
/// `0xFFFF_FFFF` and decodes back to `-1`. The server emits the same bits
/// (its encoder uses `u32::MAX` which rolls over to the same `i32::-1`).
const CHANNEL_INPUT_SOURCE_UNASSIGNED: i64 = -1;

/// Verbatim wire bytes for the screen-wake message ([`Command::ScreenTouched`]).
///
/// This is NOT a well-formed `propertyChanged` frame and so cannot go through
/// [`change_frame::encode_property_changed`]: it is the `propertyChanged`
/// header (`changeType=1`, then `compressedInt(1)` for nLevels, then a path
/// num-bytes prefix `0x01`) followed *directly* by the property name with no
/// path value and no var value. The device special-cases it. Decoding it
/// through the generic codec would swallow the first name byte as the path
/// value, so it is emitted as a fixed literal. It is layout-independent (a
/// global "wake the display" request), so there is nothing to discover.
const SCREEN_TOUCHED_FRAME: [u8; 18] = [
    0x01, // changeType = PROPERTY_CHANGED
    0x01, 0x01, // compressedInt(1) = nLevels
    0x01, // path[0] num-bytes prefix (the name bytes follow with no value)
    b's', b'c', b'r', b'e', b'e', b'n', b'T', b'o', b'u', b'c', b'h', b'e', b'd', 0x00,
];

/// Root-child index that owns `powerOffRequest` on RODECaster Pro II firmware
/// 1.7.3 (empirically captured). Unlike the channel/mix/fader families, this
/// node is not part of a discoverable run in the fullSync, so it is pinned
/// here rather than derived from [`Layout`]. Revisit if a newer firmware (or
/// the Duo) addresses power-off differently.
const POWER_OFF_NODE_INDEX: u32 = 15;

/// Path offset for a CallMe routing request. CallMe return channels are
/// addressed *outside* the regular mix matrix (their sources sit past
/// `Layout::source_count`, so `mix_cell_path` cannot reach them). The device
/// instead accepts a dedicated single-level request path
/// `(source_index << 8) | (CALLME_MIX_PATH_OFFSET + mix)` carrying
/// `mixLinkRequest` / `mixUnlinkRequest`. Confirmed against firmware 1.7.3.
const CALLME_MIX_PATH_OFFSET: u32 = 4;

/// Outgoing Rodecaster command.
///
/// Each variant addresses a logical entity (fader strip, mix matrix cell);
/// [`Command::encode`] resolves the entity through the [`Layout`] into the
/// concrete wire path, so this enum has zero knowledge of `0x1C`, `+62`, or
/// any other firmware-specific position constant.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum Command {
    /// Set the output mute on a fader strip.
    SetFaderMute { fader: u8, mute: bool },
    /// Set the cue (pre-fader-listen) enable on a fader strip.
    SetFaderCue { fader: u8, enable: bool },
    /// Set a virtual fader's level (0..127 MIDI scale).
    /// Physical fader levels come from hardware over MIDI/UART, not this path.
    SetFaderLevel { fader: u8, level: u8 },
    /// Assign (or clear, with `None`) the input source for a fader strip.
    AssignFaderSource { fader: u8, source: Option<u8> },
    /// Enable or disable a routing matrix cell.
    SetMixDisabled { source: u8, mix: u8, disabled: bool },
    /// Link a routing matrix cell. The device requires two packets in order:
    /// enable first (`mixDisabled=false`), then `mixLinkRequest`. `encode`
    /// returns both.
    LinkMix { source: u8, mix: u8 },
    /// Unlink a routing matrix cell.
    UnlinkMix { source: u8, mix: u8 },
    /// Wake the device display. A fixed, layout-independent message; see
    /// [`SCREEN_TOUCHED_FRAME`].
    ScreenTouched,
    /// Request the device power off.
    PowerOff,
    /// Link a CallMe return channel into a mix. `source` is the device
    /// protocol source id of the CallMe channel (16/17/18 on firmware 1.7.3).
    /// CallMe uses a dedicated request address outside the mix matrix, so this
    /// sends a single `mixLinkRequest` (no preceding enable, unlike
    /// [`Command::LinkMix`]).
    LinkCallMe { source: u8, mix: u8 },
    /// Unlink a CallMe return channel from a mix. See [`Command::LinkCallMe`].
    UnlinkCallMe { source: u8, mix: u8 },
}

impl Command {
    /// Encode this command into one or more change-frame payloads to send in
    /// order. `Err(EncodeError::OutOfRange)` if the addressed entity does not
    /// exist in the layout (e.g., fader index past `Layout::fader_count`).
    ///
    /// Wrap each payload in a [`crate::frame::Packet`] for the transport.
    pub fn encode(&self, layout: &Layout) -> Result<Vec<Vec<u8>>, EncodeError> {
        match self {
            Command::SetFaderMute { fader, mute } => {
                let path = channel_path(layout, *fader)?;
                Ok(vec![change_frame::encode_property_changed(
                    &path,
                    "channelOutputMute",
                    &Value::Bool(*mute),
                )])
            }
            Command::SetFaderCue { fader, enable } => {
                let path = channel_path(layout, *fader)?;
                Ok(vec![change_frame::encode_property_changed(
                    &path,
                    "channelCueEnable",
                    &Value::Bool(*enable),
                )])
            }
            Command::SetFaderLevel { fader, level } => {
                let path = fader_path(layout, *fader)?;
                Ok(vec![change_frame::encode_property_changed(
                    &path,
                    "faderLevel",
                    &Value::Int(*level as i64),
                )])
            }
            Command::AssignFaderSource { fader, source } => {
                let path = channel_path(layout, *fader)?;
                let source_value = source
                    .map(|s| s as i64)
                    .unwrap_or(CHANNEL_INPUT_SOURCE_UNASSIGNED);
                Ok(vec![change_frame::encode_property_changed(
                    &path,
                    "channelInputSource",
                    &Value::Int(source_value),
                )])
            }
            Command::SetMixDisabled {
                source,
                mix,
                disabled,
            } => {
                let path = mix_path(layout, *source, *mix)?;
                Ok(vec![change_frame::encode_property_changed(
                    &path,
                    "mixDisabled",
                    &Value::Bool(*disabled),
                )])
            }
            Command::LinkMix { source, mix } => {
                let path = mix_path(layout, *source, *mix)?;
                Ok(vec![
                    // Enable first so a previously-Disabled cell receives the link.
                    change_frame::encode_property_changed(
                        &path,
                        "mixDisabled",
                        &Value::Bool(false),
                    ),
                    change_frame::encode_property_changed(
                        &path,
                        "mixLinkRequest",
                        &Value::Binary(MIX_LINK_REQUEST_BLOB.to_vec()),
                    ),
                ])
            }
            Command::UnlinkMix { source, mix } => {
                let path = mix_path(layout, *source, *mix)?;
                Ok(vec![change_frame::encode_property_changed(
                    &path,
                    "mixUnlinkRequest",
                    &Value::Binary(MIX_LINK_REQUEST_BLOB.to_vec()),
                )])
            }
            Command::ScreenTouched => Ok(vec![SCREEN_TOUCHED_FRAME.to_vec()]),
            Command::PowerOff => Ok(vec![change_frame::encode_property_changed(
                &[POWER_OFF_NODE_INDEX],
                "powerOffRequest",
                &Value::Bool(true),
            )]),
            Command::LinkCallMe { source, mix } => Ok(vec![change_frame::encode_property_changed(
                &callme_request_path(*source, *mix),
                "mixLinkRequest",
                &Value::Binary(MIX_LINK_REQUEST_BLOB.to_vec()),
            )]),
            Command::UnlinkCallMe { source, mix } => {
                Ok(vec![change_frame::encode_property_changed(
                    &callme_request_path(*source, *mix),
                    "mixUnlinkRequest",
                    &Value::Binary(MIX_LINK_REQUEST_BLOB.to_vec()),
                )])
            }
        }
    }
}

/// Single-level request path for a CallMe routing cell. See
/// [`CALLME_MIX_PATH_OFFSET`].
fn callme_request_path(source: u8, mix: u8) -> Vec<u32> {
    vec![((source as u32) << 8) | (CALLME_MIX_PATH_OFFSET + mix as u32)]
}

fn channel_path(layout: &Layout, fader: u8) -> Result<Vec<u32>, EncodeError> {
    layout.channel_path(fader).ok_or(EncodeError::OutOfRange {
        what: "fader",
        index: fader as u32,
        bound: layout.channel_count() as u32,
    })
}

fn fader_path(layout: &Layout, fader: u8) -> Result<Vec<u32>, EncodeError> {
    layout.fader_path(fader).ok_or(EncodeError::OutOfRange {
        what: "fader",
        index: fader as u32,
        bound: layout.fader_count() as u32,
    })
}

fn mix_path(layout: &Layout, source: u8, mix: u8) -> Result<Vec<u32>, EncodeError> {
    layout
        .mix_cell_path(source, mix)
        .ok_or(EncodeError::MixCellOutOfRange {
            source,
            mix,
            source_bound: layout.source_count(),
            mix_bound: layout.mix_count_per_source(),
        })
}

#[derive(Debug, Clone, PartialEq)]
pub enum EncodeError {
    OutOfRange {
        what: &'static str,
        index: u32,
        bound: u32,
    },
    MixCellOutOfRange {
        source: u8,
        mix: u8,
        source_bound: u8,
        mix_bound: u8,
    },
}

impl std::fmt::Display for EncodeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EncodeError::OutOfRange { what, index, bound } => {
                write!(f, "{what} index {index} out of range (bound {bound})")
            }
            EncodeError::MixCellOutOfRange {
                source,
                mix,
                source_bound,
                mix_bound,
            } => write!(
                f,
                "mix cell ({source}, {mix}) out of range (sources {source_bound}, mixes {mix_bound})"
            ),
        }
    }
}

impl std::error::Error for EncodeError {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::change_frame::{decode, ChangeFrame};
    use crate::valuetree::Node;

    /// Build a small synthetic fullSync tree (PHYSICALINTERFACE at an unusual
    /// position to prove no hardcoded constants).
    fn synthetic_root() -> Node {
        fn n(name: &str) -> Node {
            Node {
                name: name.to_string(),
                properties: vec![],
                children: vec![],
            }
        }
        fn nc(name: &str, children: Vec<Node>) -> Node {
            Node {
                name: name.to_string(),
                properties: vec![],
                children,
            }
        }
        let phys = nc(
            "PHYSICALINTERFACE",
            vec![n("HEADER"), n("FADER"), n("FADER"), n("FADER")],
        );
        let mut children = vec![
            n("OTHER"),
            phys, // physical_interface_idx = 1
            n("CHANNEL"),
            n("CHANNEL"),
            n("CHANNEL"),
        ];
        for _ in 0..26 {
            children.push(n("MIX"));
        }
        nc("DEVICE", children)
    }

    fn layout() -> Layout {
        Layout::from_full_sync(&synthetic_root()).unwrap()
    }

    #[test]
    fn set_fader_mute_encodes_juce_property_changed() {
        let l = layout();
        let bytes = Command::SetFaderMute {
            fader: 1,
            mute: true,
        }
        .encode(&l)
        .unwrap();
        assert_eq!(bytes.len(), 1);

        // Decode through the JUCE change-frame codec and check addressing.
        let frame = decode(&bytes[0]).expect("decodes");
        match frame {
            ChangeFrame::PropertyChanged { path, name, value } => {
                // CHANNEL at root index 3 (= first_channel=3 + fader=1)
                assert_eq!(path, l.channel_path(1).unwrap());
                assert_eq!(name, "channelOutputMute");
                assert_eq!(value, Value::Bool(true));
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn set_fader_level_uses_two_level_path_through_physical_interface() {
        let l = layout();
        let bytes = Command::SetFaderLevel {
            fader: 2,
            level: 75,
        }
        .encode(&l)
        .unwrap();
        let frame = decode(&bytes[0]).unwrap();
        match frame {
            ChangeFrame::PropertyChanged { path, name, value } => {
                // [physical_interface_idx=1, first_fader_in_phys=1 + 2 = 3]
                assert_eq!(path, vec![1, 3]);
                assert_eq!(name, "faderLevel");
                assert_eq!(value, Value::Int(75));
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn assign_fader_source_some_and_none() {
        let l = layout();

        let some = Command::AssignFaderSource {
            fader: 0,
            source: Some(5),
        }
        .encode(&l)
        .unwrap();
        let frame = decode(&some[0]).unwrap();
        match frame {
            ChangeFrame::PropertyChanged { name, value, .. } => {
                assert_eq!(name, "channelInputSource");
                assert_eq!(value, Value::Int(5));
            }
            _ => panic!("wrong variant"),
        }

        let none = Command::AssignFaderSource {
            fader: 0,
            source: None,
        }
        .encode(&l)
        .unwrap();
        let frame = decode(&none[0]).unwrap();
        match frame {
            ChangeFrame::PropertyChanged { value, .. } => {
                // `-1` as i32, sign-extended through i64.
                assert_eq!(value, Value::Int(CHANNEL_INPUT_SOURCE_UNASSIGNED));
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn set_mix_disabled_addresses_source_major_cell() {
        let l = layout();
        let bytes = Command::SetMixDisabled {
            source: 1,
            mix: 5,
            disabled: true,
        }
        .encode(&l)
        .unwrap();
        let frame = decode(&bytes[0]).unwrap();
        match frame {
            ChangeFrame::PropertyChanged { path, name, value } => {
                assert_eq!(path, l.mix_cell_path(1, 5).unwrap());
                assert_eq!(name, "mixDisabled");
                assert_eq!(value, Value::Bool(true));
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn link_mix_emits_enable_then_link_request_in_order() {
        let l = layout();
        let bytes = Command::LinkMix { source: 0, mix: 3 }.encode(&l).unwrap();
        assert_eq!(bytes.len(), 2, "link emits two payloads");

        let first = decode(&bytes[0]).unwrap();
        let second = decode(&bytes[1]).unwrap();

        match first {
            ChangeFrame::PropertyChanged { name, value, .. } => {
                assert_eq!(name, "mixDisabled");
                assert_eq!(value, Value::Bool(false), "first packet enables the cell");
            }
            _ => panic!("first packet wrong variant"),
        }
        match second {
            ChangeFrame::PropertyChanged { name, value, .. } => {
                assert_eq!(name, "mixLinkRequest");
                assert_eq!(value, Value::Binary(MIX_LINK_REQUEST_BLOB.to_vec()));
            }
            _ => panic!("second packet wrong variant"),
        }
    }

    #[test]
    fn unlink_mix_emits_single_unlink_request() {
        let l = layout();
        let bytes = Command::UnlinkMix { source: 0, mix: 3 }.encode(&l).unwrap();
        assert_eq!(bytes.len(), 1);
        let frame = decode(&bytes[0]).unwrap();
        match frame {
            ChangeFrame::PropertyChanged { name, value, .. } => {
                assert_eq!(name, "mixUnlinkRequest");
                assert_eq!(value, Value::Binary(MIX_LINK_REQUEST_BLOB.to_vec()));
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn out_of_range_fader_returns_error_not_panic() {
        let l = layout();
        let err = Command::SetFaderMute {
            fader: 99,
            mute: true,
        }
        .encode(&l)
        .unwrap_err();
        match err {
            EncodeError::OutOfRange { what, index, .. } => {
                assert_eq!(what, "fader");
                assert_eq!(index, 99);
            }
            _ => panic!("wrong error variant"),
        }
    }

    #[test]
    fn out_of_range_mix_cell_returns_error_not_panic() {
        let l = layout();
        let err = Command::SetMixDisabled {
            source: 99,
            mix: 0,
            disabled: true,
        }
        .encode(&l)
        .unwrap_err();
        match err {
            EncodeError::MixCellOutOfRange { source, .. } => assert_eq!(source, 99),
            _ => panic!("wrong error variant"),
        }
    }

    /// Critical: the same Command encodes differently when the Layout's bases
    /// differ. Proves Command::encode is layout-driven, not constant-driven.
    #[test]
    fn encoding_depends_on_layout_not_constants() {
        fn n(name: &str) -> Node {
            Node {
                name: name.to_string(),
                properties: vec![],
                children: vec![],
            }
        }
        fn nc(name: &str, children: Vec<Node>) -> Node {
            Node {
                name: name.to_string(),
                properties: vec![],
                children,
            }
        }

        // Tree A: CHANNEL at root index 3.
        let phys = nc("PHYSICALINTERFACE", vec![n("FADER"), n("FADER")]);
        let mut a_children = vec![phys.clone(), n("X"), n("Y"), n("CHANNEL"), n("CHANNEL")];
        for _ in 0..13 {
            a_children.push(n("MIX"));
        }
        let layout_a = Layout::from_full_sync(&nc("DEVICE", a_children)).unwrap();

        // Tree B: CHANNEL at root index 5 (more leading siblings).
        let mut b_children = vec![
            phys,
            n("X"),
            n("Y"),
            n("Z"),
            n("W"),
            n("CHANNEL"),
            n("CHANNEL"),
        ];
        for _ in 0..13 {
            b_children.push(n("MIX"));
        }
        let layout_b = Layout::from_full_sync(&nc("DEVICE", b_children)).unwrap();

        let cmd = Command::SetFaderMute {
            fader: 0,
            mute: true,
        };
        let a = cmd.encode(&layout_a).unwrap();
        let b = cmd.encode(&layout_b).unwrap();

        // Same Command, but DIFFERENT wire paths -> different bytes.
        assert_ne!(
            a, b,
            "Command bytes must reflect Layout differences (proves no hardcoded base)"
        );

        // Decode each, verify they address the right CHANNEL in their tree.
        let fa = decode(&a[0]).unwrap();
        let fb = decode(&b[0]).unwrap();
        let path_a = match fa {
            ChangeFrame::PropertyChanged { path, .. } => path,
            _ => panic!(),
        };
        let path_b = match fb {
            ChangeFrame::PropertyChanged { path, .. } => path,
            _ => panic!(),
        };
        assert_eq!(path_a, vec![3]);
        assert_eq!(path_b, vec![5]);
    }

    // --- Frozen wire-byte goldens for the layout-independent / special-path
    // commands. The var-value byte sequences (Bool, Binary) are the exact
    // frames pinned by `juce_var::tests::write_matches_known_juce_frames`.

    #[test]
    fn screen_touched_golden_bytes() {
        let bytes = Command::ScreenTouched.encode(&layout()).unwrap();
        assert_eq!(bytes.len(), 1);
        // Header (changeType + nLevels=1 + path num-bytes prefix) then the
        // name with NO path value and NO var value. Not a clean propertyChanged.
        let mut expected = vec![0x01, 0x01, 0x01, 0x01];
        expected.extend_from_slice(b"screenTouched\0");
        assert_eq!(bytes[0], expected);
    }

    #[test]
    fn power_off_golden_bytes() {
        let bytes = Command::PowerOff.encode(&layout()).unwrap();
        assert_eq!(bytes.len(), 1);
        // propertyChanged, path=[15], "powerOffRequest", var Bool(true).
        let mut expected = vec![0x01, 0x01, 0x01, 0x01, 0x0f];
        expected.extend_from_slice(b"powerOffRequest\0");
        expected.extend_from_slice(&[0x01, 0x01, 0x02]); // var Bool(true)
        assert_eq!(bytes[0], expected);
    }

    #[test]
    fn link_callme_golden_bytes() {
        // source = 16 (CallMe1 protocol index), mix = 0.
        let bytes = Command::LinkCallMe { source: 16, mix: 0 }
            .encode(&layout())
            .unwrap();
        assert_eq!(bytes.len(), 1);
        // path[0] = (16<<8)|(4+0) = 4100 -> 2-byte compint `02 04 10`.
        let mut expected = vec![0x01, 0x01, 0x01, 0x02, 0x04, 0x10];
        expected.extend_from_slice(b"mixLinkRequest\0");
        expected.extend_from_slice(&[0x01, 0x07, 0x08, 0x01, 0x01, 0x02, 0x01, 0x01, 0x02]); // var Binary blob
        assert_eq!(bytes[0], expected);
    }

    #[test]
    fn unlink_callme_golden_bytes() {
        // source = 17 (CallMe2 protocol index), mix = 2.
        let bytes = Command::UnlinkCallMe { source: 17, mix: 2 }
            .encode(&layout())
            .unwrap();
        assert_eq!(bytes.len(), 1);
        // path[0] = (17<<8)|(4+2) = 4358 -> 2-byte compint `02 06 11`.
        let mut expected = vec![0x01, 0x01, 0x01, 0x02, 0x06, 0x11];
        expected.extend_from_slice(b"mixUnlinkRequest\0");
        expected.extend_from_slice(&[0x01, 0x07, 0x08, 0x01, 0x01, 0x02, 0x01, 0x01, 0x02]); // var Binary blob
        assert_eq!(bytes[0], expected);
    }
}
