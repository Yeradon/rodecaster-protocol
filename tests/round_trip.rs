//! End-to-end round-trip integration tests.
//!
//! Build a synthetic fullSync tree, derive a [`Layout`] from it, then verify
//! every typed [`Command`] survives the full pipeline:
//!
//! ```text
//!   Command::encode(&Layout)
//!     -> change-frame payload bytes
//!     -> change_frame::decode
//!     -> decode_event(payload, &Layout)
//!     -> DeviceEvent with the original logical address
//! ```
//!
//! These exercise the *integration* between the layers; per-module unit tests
//! cover each layer in isolation.

use rodecaster_protocol::{
    change_frame::{decode as decode_change_frame, encode_property_changed, ChangeFrame},
    decode_event, Command, DeviceEvent, Fader, Layout, MixOutput, Node, Property, Source, Value,
};

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

/// Synthetic fullSync that puts the addressable families at NON-default
/// positions, so every test below fails if any code accidentally hardcodes
/// 0x1C, 0x04, or 62.
fn synthetic_root() -> Node {
    let phys = nc(
        "PHYSICALINTERFACE",
        vec![
            n("HEADER"),
            np("FADER", vec![prop("faderLevel", Value::Int(50))]),
            np("FADER", vec![prop("faderLevel", Value::Int(60))]),
            np("FADER", vec![prop("faderLevel", Value::Int(70))]),
            n("FOOTER"),
        ],
    );

    let mut children = vec![
        n("OTHER1"),
        n("OTHER2"),
        phys, // physical_interface_idx = 2 (NOT 0)
        n("OTHER3"),
        np(
            "CHANNEL",
            vec![
                prop("channelOutputMute", Value::Bool(false)),
                prop("channelCueEnable", Value::Bool(false)),
            ],
        ),
        np(
            "CHANNEL",
            vec![prop("channelOutputMute", Value::Bool(true))],
        ),
        np(
            "CHANNEL",
            vec![prop("channelOutputMute", Value::Bool(false))],
        ),
        n("OTHER4"),
    ];
    // 26 MIX nodes => 2 sources × 13 mixes.
    for i in 0..26 {
        children.push(np(
            "MIX",
            vec![prop(
                "mixLevelWithAnchor",
                Value::String(format!("0.5|{}", 0.1 * (i % 13) as f32)),
            )],
        ));
    }
    nc("DEVICE", children)
}

fn layout() -> Layout {
    Layout::from_full_sync(&synthetic_root()).unwrap()
}

#[test]
fn round_trip_set_fader_mute() {
    let l = layout();
    let cmd = Command::SetFaderMute {
        fader: Fader::Physical2,
        mute: true,
    };
    let payloads = cmd.encode(&l).unwrap();
    assert_eq!(payloads.len(), 1);

    let event = decode_event(&payloads[0], &l).unwrap();
    assert_eq!(
        event,
        DeviceEvent::FaderMuteChanged {
            fader: Fader::Physical2,
            muted: true,
        }
    );
}

#[test]
fn round_trip_set_fader_cue() {
    let l = layout();
    let cmd = Command::SetFaderCue {
        fader: Fader::Physical1,
        enable: true,
    };
    let payloads = cmd.encode(&l).unwrap();
    let event = decode_event(&payloads[0], &l).unwrap();
    assert_eq!(
        event,
        DeviceEvent::FaderCueChanged {
            fader: Fader::Physical1,
            enabled: true,
        }
    );
}

#[test]
fn round_trip_set_fader_level_through_two_level_path() {
    let l = layout();
    let cmd = Command::SetFaderLevel {
        fader: Fader::Physical3,
        level: 100,
    };
    let payloads = cmd.encode(&l).unwrap();
    let event = decode_event(&payloads[0], &l).unwrap();
    assert_eq!(
        event,
        DeviceEvent::FaderLevelChanged {
            fader: Fader::Physical3,
            level: 100,
        }
    );
}

#[test]
fn round_trip_set_mix_disabled() {
    let l = layout();
    let cmd = Command::SetMixDisabled {
        source: Source::Combo2,
        mix: MixOutput::Usb1,
        disabled: true,
    };
    let payloads = cmd.encode(&l).unwrap();
    let event = decode_event(&payloads[0], &l).unwrap();
    assert_eq!(
        event,
        DeviceEvent::MixDisabledChanged {
            source: Source::Combo2,
            mix: MixOutput::Usb1,
            disabled: true,
        }
    );
}

#[test]
fn round_trip_link_mix_emits_two_decodable_events() {
    let l = layout();
    let cmd = Command::LinkMix {
        source: Source::Combo1,
        mix: MixOutput::Speaker,
    };
    let payloads = cmd.encode(&l).unwrap();
    assert_eq!(payloads.len(), 2);

    // First payload: enable (mixDisabled = false). Decodes as MixDisabledChanged.
    let first = decode_event(&payloads[0], &l).unwrap();
    assert_eq!(
        first,
        DeviceEvent::MixDisabledChanged {
            source: Source::Combo1,
            mix: MixOutput::Speaker,
            disabled: false,
        }
    );

    // Second payload: mixLinkRequest. Not a typed event in v0.2; surfaces as Unknown.
    let second = decode_event(&payloads[1], &l).unwrap();
    match second {
        DeviceEvent::Unknown { prop_name, .. } => {
            assert_eq!(prop_name, "mixLinkRequest");
        }
        other => panic!("expected Unknown for mixLinkRequest, got {other:?}"),
    }
}

#[test]
fn channel_input_source_is_asymmetric_write_stride1_echo_stride6() {
    let l = layout();
    // channelInputSource is asymmetric on firmware 1.7.3: AssignFaderSource
    // WRITES at stride 1 (channel_path), but the device ECHOES at stride 6 from
    // first_channel. Encode stays honest (one write frame); decode resolves the
    // echo addressing to the originating fader.
    let write = Command::AssignFaderSource {
        fader: Fader::Physical1,
        source: Some(Source::Combo2_3),
    }
    .encode(&l)
    .unwrap();
    assert_eq!(write.len(), 1);

    let echo = encode_property_changed(
        &[l.first_channel() + 6 * 2],
        "channelInputSource",
        &Value::Int(5),
    );
    let event = decode_event(&echo, &l).unwrap();
    assert_eq!(
        event,
        DeviceEvent::FaderAssignmentChanged {
            fader: Fader::Physical3,
            source: Some(Source::Combo2_3),
        }
    );
}

#[test]
fn full_sync_path_yields_initial_state_for_synthetic_tree() {
    let root = synthetic_root();
    let l = Layout::from_full_sync(&root).unwrap();

    // Build a synthetic FullSync change-frame: [0x02] + ValueTree stream.
    // For this test we use parse_valuetree directly + extract_initial_state
    // through decode_event by handcrafting the FullSync payload.
    let mut payload = vec![0x02];
    write_value_tree(&root, &mut payload);

    let event = decode_event(&payload, &l).unwrap();
    let initial = match event {
        DeviceEvent::InitialState(events) => events,
        other => panic!("expected InitialState, got {other:?}"),
    };

    // 3 fader levels + 3 mute + 1 cue + 26 mix levels = 33 events.
    assert_eq!(initial.len(), 33);

    // First three should be fader-level inits from PHYSICALINTERFACE/FADER.
    assert_eq!(
        initial[0],
        DeviceEvent::FaderLevelChanged {
            fader: Fader::Physical1,
            level: 50,
        }
    );
    assert_eq!(
        initial[1],
        DeviceEvent::FaderLevelChanged {
            fader: Fader::Physical2,
            level: 60,
        }
    );
    assert_eq!(
        initial[2],
        DeviceEvent::FaderLevelChanged {
            fader: Fader::Physical3,
            level: 70,
        }
    );
}

#[test]
fn structural_change_invalidates_layout() {
    let l = layout();
    // childRemoved: [4] [path=[2]] [oldIndex=3]
    let payload = [0x04, 0x01, 0x01, 0x01, 0x02, 0x01, 0x03];
    let event = decode_event(&payload, &l).unwrap();
    assert_eq!(event, DeviceEvent::LayoutInvalidated);
}

#[test]
fn full_round_trip_through_transport_packet() {
    use rodecaster_protocol::Packet;
    let l = layout();
    let cmd = Command::SetFaderMute {
        fader: Fader::Physical1,
        mute: true,
    };
    let payloads = cmd.encode(&l).unwrap();

    // Wrap in transport Packet.
    let pkt = Packet::new(payloads[0].clone());
    let wire = pkt.to_bytes();

    // Unwrap back.
    let (received_pkt, used) = Packet::from_bytes(&wire).unwrap();
    assert_eq!(used, wire.len());

    let event = decode_event(&received_pkt.payload, &l).unwrap();
    assert_eq!(
        event,
        DeviceEvent::FaderMuteChanged {
            fader: Fader::Physical1,
            muted: true,
        }
    );
}

/// Verify the same Command produces different wire bytes when the Layout
/// differs, all the way through the round-trip pipeline. Catches any hidden
/// hardcoded position.
#[test]
fn round_trip_addresses_track_layout_not_constants() {
    // Tree A: CHANNEL at root index 4.
    let phys = nc("PHYSICALINTERFACE", vec![n("FADER"), n("FADER")]);
    let mut a_children = vec![
        phys.clone(),
        n("X"),
        n("Y"),
        n("Z"),
        n("CHANNEL"),
        n("CHANNEL"),
    ];
    for _ in 0..13 {
        a_children.push(n("MIX"));
    }
    let layout_a = Layout::from_full_sync(&nc("DEVICE", a_children)).unwrap();

    // Tree B: CHANNEL at root index 7.
    let mut b_children = vec![
        phys,
        n("X"),
        n("Y"),
        n("Z"),
        n("W1"),
        n("W2"),
        n("W3"),
        n("CHANNEL"),
        n("CHANNEL"),
    ];
    for _ in 0..13 {
        b_children.push(n("MIX"));
    }
    let layout_b = Layout::from_full_sync(&nc("DEVICE", b_children)).unwrap();

    let cmd = Command::SetFaderMute {
        fader: Fader::Physical1,
        mute: true,
    };

    let a = cmd.encode(&layout_a).unwrap();
    let b = cmd.encode(&layout_b).unwrap();
    assert_ne!(a, b, "wire bytes must differ when Layout differs");

    // Both must decode back to fader 0 under their own Layout.
    assert_eq!(
        decode_event(&a[0], &layout_a).unwrap(),
        DeviceEvent::FaderMuteChanged {
            fader: Fader::Physical1,
            muted: true,
        }
    );
    assert_eq!(
        decode_event(&b[0], &layout_b).unwrap(),
        DeviceEvent::FaderMuteChanged {
            fader: Fader::Physical1,
            muted: true,
        }
    );

    // And critically, decoding A's bytes under B's Layout produces a different
    // (or Unknown) result because the addressing context differs.
    let cross = decode_event(&a[0], &layout_b).unwrap();
    assert_ne!(
        cross,
        DeviceEvent::FaderMuteChanged {
            fader: Fader::Physical1,
            muted: true,
        },
        "different Layout must NOT resolve A's path the same way"
    );
}

/// Verify change_frame::decode reads exactly the bytes the encoder writes.
#[test]
fn change_frame_codec_round_trip_via_encoder() {
    use rodecaster_protocol::change_frame::encode_property_changed;

    let cases = vec![
        (vec![28u32], "channelOutputMute", Value::Bool(true)),
        (
            vec![217],
            "mixLevelWithAnchor",
            Value::String("0.5|0.5".to_string()),
        ),
        (vec![0, 7], "faderLevel", Value::Int(75)),
        (vec![0], "rootProperty", Value::Undefined),
    ];

    for (path, name, value) in cases {
        let bytes = encode_property_changed(&path, name, &value);
        let frame = decode_change_frame(&bytes).expect("decodes");
        match frame {
            ChangeFrame::PropertyChanged {
                path: p,
                name: n,
                value: v,
            } => {
                assert_eq!(p, path);
                assert_eq!(n, name);
                assert_eq!(v, value);
            }
            other => panic!("expected PropertyChanged, got {other:?}"),
        }
    }
}

// Helpers

/// Serialize a Node back to JUCE `ValueTree::writeToStream` bytes (name,
/// prop count, props, child count, children, recursive). Mirror of the
/// crate's parse_node. Used by the FullSync integration test to build a
/// fullSync payload from a Node tree.
fn write_value_tree(node: &Node, out: &mut Vec<u8>) {
    out.extend_from_slice(node.name.as_bytes());
    out.push(0);
    write_compressed_int(out, node.properties.len() as i64);
    for p in &node.properties {
        out.extend_from_slice(p.name.as_bytes());
        out.push(0);
        p.value.write_to_stream(out);
    }
    write_compressed_int(out, node.children.len() as i64);
    for c in &node.children {
        write_value_tree(c, out);
    }
}

fn write_compressed_int(out: &mut Vec<u8>, value: i64) {
    let negative = value < 0;
    let mut magnitude = value.unsigned_abs();
    let mut bytes = [0u8; 4];
    let mut num_bytes = 0usize;
    while magnitude != 0 && num_bytes < 4 {
        bytes[num_bytes] = (magnitude & 0xff) as u8;
        magnitude >>= 8;
        num_bytes += 1;
    }
    let size_byte = num_bytes as u8 | if negative { 0x80 } else { 0 };
    out.push(size_byte);
    out.extend_from_slice(&bytes[..num_bytes]);
}
