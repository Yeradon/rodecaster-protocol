//! Typed Rodecaster device events.
//!
//! `DeviceEvent` is the inbound vocabulary the consumer receives from the
//! device. [`decode_event`] turns one wire payload into one typed event,
//! routing through [`crate::change_frame`] and resolving paths through a
//! [`crate::Layout`] discovered from the device's fullSync.
//!
//! ## Asymmetric echo addressing
//!
//! Two echoes the device emits are addressed differently from their write
//! paths, empirically derived on firmware 1.7.3:
//!
//! - `channelInputSource` ([`DeviceEvent::FaderAssignmentChanged`]): written at
//!   stride 1 from `first_channel`, but echoed back at **stride 6**
//!   (`0x1C`=fader0, `0x22`=fader1, ...). [`decode_property`] resolves the echo
//!   with that stride.
//! - `encoderSignal` ([`DeviceEvent::FaderTouched`]): a single-level path whose
//!   value is the raw fader index (no base offset).
//!
//! Both formulas reproduce the behaviour the reference server ran in
//! production; a future capture on newer firmware may refine them.

use crate::change_frame::{decode as decode_frame, ChangeFrame};
use crate::juce_var::Value;
use crate::layout::Layout;
use crate::valuetree::Node;

/// Typed event decoded from one wire payload.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum DeviceEvent {
    /// A fullSync arrived. The contained events are the initial state, in
    /// the order the consumer should apply them to its domain model.
    /// Equivalent to calling [`extract_initial_state`] on the parsed root.
    InitialState(Vec<DeviceEvent>),

    FaderMuteChanged {
        fader: u8,
        muted: bool,
    },
    FaderCueChanged {
        fader: u8,
        enabled: bool,
    },
    /// Virtual fader level (0..127 MIDI scale).
    FaderLevelChanged {
        fader: u8,
        level: u8,
    },
    /// A fader strip was touched (the device's `encoderSignal`). The wire
    /// addresses it by raw fader index (single-level path, no base offset).
    FaderTouched {
        fader: u8,
    },
    /// A fader's input-source assignment changed (`channelInputSource` echo,
    /// resolved at the stride-6 echo addressing — see module docs). `source`
    /// is `None` when the slot was unassigned (wire value < 0).
    FaderAssignmentChanged {
        fader: u8,
        source: Option<u8>,
    },

    /// `mixLevelWithAnchor` carries two fields, `anchor|value`. `anchor` is the
    /// configured per-route matrix level; `value` is the live fader-tracked
    /// level (equal to `anchor` when the wire sends a single number).
    MixLevelChanged {
        source: u8,
        mix: u8,
        anchor: f32,
        value: f32,
    },
    MixMuteChanged {
        source: u8,
        mix: u8,
        muted: bool,
    },
    MixLinkChanged {
        source: u8,
        mix: u8,
        linked: bool,
    },
    MixDisabledChanged {
        source: u8,
        mix: u8,
        disabled: bool,
    },

    /// Tree topology changed (`childAdded` / `childRemoved` / `childMoved`).
    /// The current [`Layout`] is potentially stale; expect or request a fresh
    /// fullSync and rebuild Layout before trusting subsequent address lookups.
    LayoutInvalidated,

    /// Property name/path didn't match any known Rodecaster event under the
    /// current [`Layout`]. The wire data is preserved so consumers can log
    /// it, react, or pattern-match on raw paths without losing data. Future
    /// firmware additions surface here rather than vanishing.
    Unknown {
        prop_name: String,
        path: Vec<u32>,
        value: Option<Value>,
    },
}

/// Decode one wire payload (the change-frame, not including transport frame)
/// into a typed event. Returns `None` if the payload isn't a recognized JUCE
/// change-frame at all.
pub fn decode_event(payload: &[u8], layout: &Layout) -> Option<DeviceEvent> {
    let frame = decode_frame(payload)?;
    Some(decode_change_frame(frame, layout))
}

fn decode_change_frame(frame: ChangeFrame, layout: &Layout) -> DeviceEvent {
    match frame {
        ChangeFrame::FullSync { root } => {
            DeviceEvent::InitialState(extract_initial_state(&root, layout))
        }
        ChangeFrame::ChildAdded { .. }
        | ChangeFrame::ChildRemoved { .. }
        | ChangeFrame::ChildMoved { .. } => DeviceEvent::LayoutInvalidated,
        ChangeFrame::PropertyChanged { path, name, value } => {
            decode_property(&path, &name, Some(value), layout)
        }
        ChangeFrame::PropertyRemoved { path, name } => decode_property(&path, &name, None, layout),
    }
}

fn decode_property(path: &[u32], name: &str, value: Option<Value>, layout: &Layout) -> DeviceEvent {
    // CHANNEL-addressed properties (single-level path).
    if let Some(fader) = layout.channel_index_from_path(path) {
        match name {
            "channelOutputMute" => {
                if let Some(Value::Bool(muted)) = value {
                    return DeviceEvent::FaderMuteChanged { fader, muted };
                }
            }
            "channelCueEnable" => {
                if let Some(Value::Bool(enabled)) = value {
                    return DeviceEvent::FaderCueChanged { fader, enabled };
                }
            }
            // "channelInputSource" is NOT handled here: its echo uses stride-6
            // addressing (not the stride-1 `channel_index_from_path`), so it is
            // resolved separately below.
            _ => {}
        }
    }

    // FADER-addressed properties (two-level path through PHYSICALINTERFACE).
    if let Some(fader) = layout.fader_index_from_path(path) {
        if name == "faderLevel" {
            if let Some(v) = &value {
                if let Some(level_i64) = v.as_int() {
                    let level = level_i64.clamp(0, 127) as u8;
                    return DeviceEvent::FaderLevelChanged { fader, level };
                }
            }
        }
    }

    // MIX-cell-addressed properties (single-level path).
    if let Some((source, mix)) = layout.mix_cell_from_path(path) {
        match name {
            "mixMute" => {
                if let Some(Value::Bool(muted)) = value {
                    return DeviceEvent::MixMuteChanged { source, mix, muted };
                }
            }
            "mixLink" => {
                if let Some(Value::Bool(linked)) = value {
                    return DeviceEvent::MixLinkChanged {
                        source,
                        mix,
                        linked,
                    };
                }
            }
            "mixDisabled" => {
                if let Some(Value::Bool(disabled)) = value {
                    return DeviceEvent::MixDisabledChanged {
                        source,
                        mix,
                        disabled,
                    };
                }
            }
            "mixLevelWithAnchor" => {
                if let Some(Value::String(s)) = &value {
                    if let Some((anchor, value)) = parse_mix_level(s) {
                        return DeviceEvent::MixLevelChanged {
                            source,
                            mix,
                            anchor,
                            value,
                        };
                    }
                }
            }
            _ => {}
        }
    }

    // encoderSignal (fader touch): single-level path whose value IS the fader
    // index (stride 1, no base offset). See module docs.
    if name == "encoderSignal" {
        if let Some(&raw) = path.first() {
            if raw < layout.fader_count() as u32 {
                return DeviceEvent::FaderTouched { fader: raw as u8 };
            }
        }
    }

    // channelInputSource echo: addressed at stride 6 from `first_channel`,
    // asymmetric with the stride-1 write path. See module docs. A wire value
    // < 0 means the slot was unassigned.
    if name == "channelInputSource" {
        if let Some(fader) = path
            .first()
            .and_then(|raw| raw.checked_sub(layout.first_channel()))
            .map(|offset| offset / 6)
            .filter(|&fader| fader < layout.channel_count() as u32)
        {
            let source = value
                .as_ref()
                .and_then(Value::as_int)
                .filter(|&s| s >= 0)
                .and_then(|s| u8::try_from(s).ok());
            return DeviceEvent::FaderAssignmentChanged {
                fader: fader as u8,
                source,
            };
        }
    }

    DeviceEvent::Unknown {
        prop_name: name.to_string(),
        path: path.to_vec(),
        value,
    }
}

/// `mixLevelWithAnchor` wire form: `anchor|value` (or a single number for
/// both). Returns `(anchor, value)`: the left/configured matrix level and the
/// right/fader-tracked level.
fn parse_mix_level(s: &str) -> Option<(f32, f32)> {
    let anchor = s.split('|').next()?.parse().ok()?;
    let value = s.split('|').next_back()?.parse().ok()?;
    Some((anchor, value))
}

/// Walk a parsed fullSync and produce the initial DeviceEvent list. Mirrors
/// the server's `extract_initial_state`, layout-driven (no hardcoded counts).
pub fn extract_initial_state(root: &Node, layout: &Layout) -> Vec<DeviceEvent> {
    let mut out = Vec::new();

    // 1. PHYSICALINTERFACE -> FADER initial levels (physical strips only).
    if let Some(phys) = root.children.iter().find(|n| n.name == "PHYSICALINTERFACE") {
        let mut fader_idx: u8 = 0;
        for child in &phys.children {
            if child.name != "FADER" {
                continue;
            }
            if let Some(level) = int_prop(child, "faderLevel") {
                out.push(DeviceEvent::FaderLevelChanged {
                    fader: fader_idx,
                    level: level.clamp(0, 127) as u8,
                });
            }
            fader_idx = fader_idx.saturating_add(1);
        }
    }

    // 2. CHANNEL initial mute/cue (one per strip, including virtuals).
    let mut channel_idx: u8 = 0;
    for child in &root.children {
        if child.name != "CHANNEL" {
            continue;
        }
        if let Some(muted) = bool_prop(child, "channelOutputMute") {
            out.push(DeviceEvent::FaderMuteChanged {
                fader: channel_idx,
                muted,
            });
        }
        if let Some(enabled) = bool_prop(child, "channelCueEnable") {
            out.push(DeviceEvent::FaderCueChanged {
                fader: channel_idx,
                enabled,
            });
        }
        // In a fullSync the assignment sits on the Nth CHANNEL positionally
        // (stride 1), unlike the stride-6 incremental echo.
        if let Some(source_i) = int_prop(child, "channelInputSource") {
            out.push(DeviceEvent::FaderAssignmentChanged {
                fader: channel_idx,
                source: if source_i < 0 {
                    None
                } else {
                    u8::try_from(source_i).ok()
                },
            });
        }
        channel_idx = channel_idx.saturating_add(1);
    }

    // 3. MIX cells initial values. Source-major: cell N has source = N/13, mix = N%13.
    let mut mix_counter: u32 = 0;
    let per_source = layout.mix_count_per_source() as u32;
    for child in &root.children {
        if child.name != "MIX" {
            continue;
        }
        let source = (mix_counter / per_source) as u8;
        let mix = (mix_counter % per_source) as u8;
        if let Some(level_s) = string_prop(child, "mixLevelWithAnchor") {
            if let Some((anchor, value)) = parse_mix_level(level_s) {
                out.push(DeviceEvent::MixLevelChanged {
                    source,
                    mix,
                    anchor,
                    value,
                });
            }
        }
        if let Some(muted) = bool_prop(child, "mixMute") {
            out.push(DeviceEvent::MixMuteChanged { source, mix, muted });
        }
        if let Some(linked) = bool_prop(child, "mixLink") {
            out.push(DeviceEvent::MixLinkChanged {
                source,
                mix,
                linked,
            });
        }
        if let Some(disabled) = bool_prop(child, "mixDisabled") {
            out.push(DeviceEvent::MixDisabledChanged {
                source,
                mix,
                disabled,
            });
        }
        mix_counter += 1;
    }

    out
}

fn int_prop(node: &Node, name: &str) -> Option<i64> {
    node.properties
        .iter()
        .find(|p| p.name == name)
        .and_then(|p| p.value.as_int())
}

fn bool_prop(node: &Node, name: &str) -> Option<bool> {
    node.properties
        .iter()
        .find(|p| p.name == name)
        .and_then(|p| p.value.as_bool())
}

fn string_prop<'a>(node: &'a Node, name: &str) -> Option<&'a str> {
    node.properties
        .iter()
        .find(|p| p.name == name)
        .and_then(|p| match &p.value {
            Value::String(s) => Some(s.as_str()),
            _ => None,
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::change_frame::encode_property_changed;
    use crate::valuetree::{Node, Property};

    fn n(name: &str) -> Node {
        Node {
            name: name.to_string(),
            properties: vec![],
            children: vec![],
        }
    }
    fn np(name: &str, properties: Vec<Property>) -> Node {
        Node {
            name: name.to_string(),
            properties,
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
    fn prop(name: &str, value: Value) -> Property {
        Property {
            name: name.to_string(),
            value,
        }
    }

    fn synthetic_root() -> Node {
        let phys = nc(
            "PHYSICALINTERFACE",
            vec![n("HEADER"), n("FADER"), n("FADER"), n("FADER")],
        );
        let mut children = vec![n("OTHER"), phys, n("CHANNEL"), n("CHANNEL"), n("CHANNEL")];
        for _ in 0..26 {
            children.push(n("MIX"));
        }
        nc("DEVICE", children)
    }

    fn layout() -> Layout {
        Layout::from_full_sync(&synthetic_root()).unwrap()
    }

    #[test]
    fn decodes_fader_mute_changed() {
        let l = layout();
        let path = l.channel_path(2).unwrap();
        let payload = encode_property_changed(&path, "channelOutputMute", &Value::Bool(true));
        let event = decode_event(&payload, &l).unwrap();
        assert_eq!(
            event,
            DeviceEvent::FaderMuteChanged {
                fader: 2,
                muted: true,
            }
        );
    }

    #[test]
    fn decodes_fader_cue_changed() {
        let l = layout();
        let path = l.channel_path(0).unwrap();
        let payload = encode_property_changed(&path, "channelCueEnable", &Value::Bool(false));
        let event = decode_event(&payload, &l).unwrap();
        assert_eq!(
            event,
            DeviceEvent::FaderCueChanged {
                fader: 0,
                enabled: false,
            }
        );
    }

    #[test]
    fn decodes_fader_level_changed_two_level_path() {
        let l = layout();
        let path = l.fader_path(1).unwrap();
        let payload = encode_property_changed(&path, "faderLevel", &Value::Int(99));
        let event = decode_event(&payload, &l).unwrap();
        assert_eq!(
            event,
            DeviceEvent::FaderLevelChanged {
                fader: 1,
                level: 99,
            }
        );
    }

    #[test]
    fn decodes_fader_level_clamps_to_midi_range() {
        let l = layout();
        let path = l.fader_path(0).unwrap();
        let payload = encode_property_changed(&path, "faderLevel", &Value::Int(500));
        let event = decode_event(&payload, &l).unwrap();
        assert_eq!(
            event,
            DeviceEvent::FaderLevelChanged {
                fader: 0,
                level: 127,
            }
        );
    }

    #[test]
    fn decodes_mix_disabled_changed() {
        let l = layout();
        let path = l.mix_cell_path(1, 5).unwrap();
        let payload = encode_property_changed(&path, "mixDisabled", &Value::Bool(true));
        let event = decode_event(&payload, &l).unwrap();
        assert_eq!(
            event,
            DeviceEvent::MixDisabledChanged {
                source: 1,
                mix: 5,
                disabled: true,
            }
        );
    }

    #[test]
    fn decodes_mix_level_anchor_split() {
        let l = layout();
        let path = l.mix_cell_path(0, 0).unwrap();
        let payload = encode_property_changed(
            &path,
            "mixLevelWithAnchor",
            &Value::String("0.3|0.7".to_string()),
        );
        let event = decode_event(&payload, &l).unwrap();
        match event {
            DeviceEvent::MixLevelChanged {
                source,
                mix,
                anchor,
                value,
            } => {
                assert_eq!(source, 0);
                assert_eq!(mix, 0);
                assert!((anchor - 0.3).abs() < 1e-4);
                assert!((value - 0.7).abs() < 1e-4);
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn decodes_encoder_signal_as_fader_touch() {
        // encoderSignal addresses the fader by raw index (stride 1, no base).
        let l = layout();
        let payload = encode_property_changed(&[1u32], "encoderSignal", &Value::Int(1));
        let event = decode_event(&payload, &l).unwrap();
        assert_eq!(event, DeviceEvent::FaderTouched { fader: 1 });
    }

    #[test]
    fn decodes_channel_input_source_echo_at_stride_6() {
        // The echo for fader N sits at first_channel + 6*N, not the stride-1
        // write path.
        let l = layout();
        let path = vec![l.first_channel() + 6 * 2];
        let payload = encode_property_changed(&path, "channelInputSource", &Value::Int(7));
        let event = decode_event(&payload, &l).unwrap();
        assert_eq!(
            event,
            DeviceEvent::FaderAssignmentChanged {
                fader: 2,
                source: Some(7),
            }
        );
    }

    #[test]
    fn channel_input_source_negative_value_is_unassigned() {
        let l = layout();
        let path = vec![l.first_channel()]; // fader 0
        let payload = encode_property_changed(&path, "channelInputSource", &Value::Int(-1));
        let event = decode_event(&payload, &l).unwrap();
        assert_eq!(
            event,
            DeviceEvent::FaderAssignmentChanged {
                fader: 0,
                source: None,
            }
        );
    }

    #[test]
    fn unknown_property_preserves_wire_data() {
        let l = layout();
        let path = l.channel_path(0).unwrap();
        let payload = encode_property_changed(&path, "futureProperty", &Value::Int(42));
        let event = decode_event(&payload, &l).unwrap();
        match event {
            DeviceEvent::Unknown {
                prop_name,
                path: p,
                value,
            } => {
                assert_eq!(prop_name, "futureProperty");
                assert_eq!(p, path);
                assert_eq!(value, Some(Value::Int(42)));
            }
            other => panic!("expected Unknown, got {other:?}"),
        }
    }

    #[test]
    fn structural_change_yields_layout_invalidated() {
        let l = layout();
        // childRemoved: [4] [path=[2]] [oldIndex=3]
        let payload = [0x04, 0x01, 0x01, 0x01, 0x02, 0x01, 0x03];
        let event = decode_event(&payload, &l).unwrap();
        assert_eq!(event, DeviceEvent::LayoutInvalidated);
    }

    #[test]
    fn full_sync_yields_initial_state_with_extracted_events() {
        let phys = nc(
            "PHYSICALINTERFACE",
            vec![
                np("FADER", vec![prop("faderLevel", Value::Int(64))]),
                np("FADER", vec![prop("faderLevel", Value::Int(100))]),
            ],
        );
        let mut children = vec![
            phys,
            np(
                "CHANNEL",
                vec![
                    prop("channelOutputMute", Value::Bool(false)),
                    prop("channelCueEnable", Value::Bool(true)),
                ],
            ),
            np(
                "CHANNEL",
                vec![prop("channelOutputMute", Value::Bool(true))],
            ),
        ];
        // 13 MIX nodes -> 1 source, mix 0..12.
        for i in 0..13 {
            children.push(np(
                "MIX",
                vec![prop(
                    "mixLevelWithAnchor",
                    Value::String(format!("0.5|{}", 0.1 * i as f32)),
                )],
            ));
        }
        let root = nc("DEVICE", children);
        let l = Layout::from_full_sync(&root).unwrap();

        let events = extract_initial_state(&root, &l);
        // 2 fader levels + 2 mute + 1 cue + 13 mix levels = 18 events
        assert_eq!(events.len(), 18);
        assert_eq!(
            events[0],
            DeviceEvent::FaderLevelChanged {
                fader: 0,
                level: 64,
            }
        );
        assert_eq!(
            events[1],
            DeviceEvent::FaderLevelChanged {
                fader: 1,
                level: 100,
            }
        );
        assert_eq!(
            events[2],
            DeviceEvent::FaderMuteChanged {
                fader: 0,
                muted: false,
            }
        );
        assert_eq!(
            events[3],
            DeviceEvent::FaderCueChanged {
                fader: 0,
                enabled: true,
            }
        );
        assert_eq!(
            events[4],
            DeviceEvent::FaderMuteChanged {
                fader: 1,
                muted: true,
            }
        );
        // Last few should be mix levels
        if let DeviceEvent::MixLevelChanged { source, mix, .. } = events[17] {
            assert_eq!(source, 0);
            assert_eq!(mix, 12);
        } else {
            panic!("expected MixLevelChanged at end");
        }
    }
}
