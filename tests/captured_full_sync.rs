//! Integration tests backed by a sanitized full sync captured from real hardware.
//!
//! Unlike the synthetic tests, this corpus fixes the complete node ordering,
//! property vocabulary, value shapes, and model-specific family counts emitted
//! by released firmware.

use rodecaster_protocol::change_frame::{decode, encode_full_sync, ChangeFrame};
use rodecaster_protocol::frame::Packet;
use rodecaster_protocol::{
    decode_event, Command, DeviceEvent, DeviceModel, Fader, Layout, ProtocolSession, SessionError,
    SessionUpdate, Value,
};

const DUO_174: &[u8] = include_bytes!("fixtures/duo/fw-1.7.4/full-sync.frame");

fn duo_fixture() -> (rodecaster_protocol::Node, Layout) {
    let (packet, consumed) = Packet::from_bytes(DUO_174).expect("valid captured TCP packet");
    assert_eq!(consumed, DUO_174.len(), "fixture contains trailing bytes");
    let ChangeFrame::FullSync { root } = decode(&packet.payload).expect("valid full sync") else {
        panic!("fixture must contain a full sync");
    };
    let layout = Layout::from_full_sync(&root).expect("layout from real Duo tree");
    (root, layout)
}

#[test]
fn duo_174_full_sync_contract() {
    let (root, layout) = duo_fixture();

    assert_eq!(layout.model(), DeviceModel::Duo);
    assert_eq!(layout.fader_count(), 9);
    assert_eq!(layout.channel_count(), 10);
    assert_eq!(layout.source_count(), 19);
    assert_eq!(layout.mix_count_per_source(), 13);
    assert_eq!(layout.input_source_count(), 19);
    assert_eq!(layout.headphone_count(), 4);
    assert_eq!(layout.effects_count(), 11);
    assert_eq!(layout.pad_count(), 21);
    assert_eq!(layout.sip_call_slots_count(), 3);
    assert_eq!(layout.sip_registration_count(), 2);

    assert_eq!(tree_size(&root), (870, 6923));
    assert_eq!(
        find_property(&root, "systemFirmwareVersion"),
        Some(&Value::String("1.7.4".to_string()))
    );
}

#[test]
fn high_level_session_discovers_duo_capabilities() {
    let (packet, _) = Packet::from_bytes(DUO_174).unwrap();
    let mut session = ProtocolSession::new();
    let SessionUpdate::Ready { initial_events } = session.ingest(&packet.payload).unwrap() else {
        panic!("full sync must ready the session");
    };

    let capabilities = session.capabilities().expect("discovered capabilities");
    assert_eq!(capabilities.model(), DeviceModel::Duo);
    assert_eq!(capabilities.firmware(), Some("1.7.4"));
    assert_eq!(capabilities.faders().len(), 9);
    assert_eq!(capabilities.sources().len(), 19);
    assert_eq!(capabilities.mix_outputs().len(), 13);
    assert!(capabilities.supports_fader(Fader::Physical4));
    assert!(!capabilities.supports_fader(Fader::Physical5));
    assert_eq!(capabilities.physical_combo_count(), 2);
    assert_eq!(capabilities.physical_headphone_count(), 2);
    assert_eq!(initial_events.len(), 3_068);

    let payloads = session
        .encode(&Command::SetFaderMute {
            fader: Fader::Physical4,
            mute: true,
        })
        .expect("command supported by captured Duo topology");
    assert_eq!(payloads.len(), 1);
    assert!(matches!(
        decode_event(&payloads[0], session.layout().unwrap()),
        Some(DeviceEvent::FaderMuteChanged {
            fader: Fader::Physical4,
            muted: true,
        })
    ));

    assert!(matches!(
        session.encode(&Command::SetFaderMute {
            fader: Fader::Physical5,
            mute: true,
        }),
        Err(SessionError::Encode(_))
    ));
}

#[test]
fn high_level_session_drops_stale_layout_after_failed_resync() {
    let (packet, _) = Packet::from_bytes(DUO_174).unwrap();
    let mut session = ProtocolSession::new();
    session.ingest(&packet.payload).unwrap();
    assert!(session.is_ready());

    let unsupported = rodecaster_protocol::Node {
        name: "DEVICE".to_string(),
        properties: vec![],
        children: vec![],
    };
    assert!(matches!(
        session.ingest(&encode_full_sync(&unsupported)),
        Err(SessionError::Layout(_))
    ));
    assert!(!session.is_ready());
    assert!(session.capabilities().is_none());
}

#[test]
fn high_level_session_invalidates_captured_layout_on_structure_change() {
    let (packet, _) = Packet::from_bytes(DUO_174).unwrap();
    let mut session = ProtocolSession::new();
    session.ingest(&packet.payload).unwrap();
    assert!(session.is_ready());

    let child_moved = [0x05, 0x00, 0x01, 0x00, 0x01, 0x01];
    assert_eq!(
        session.ingest(&child_moved).unwrap(),
        SessionUpdate::NeedsFullSync
    );
    assert!(!session.is_ready());
    assert!(session.capabilities().is_none());
}

#[test]
fn duo_174_all_discovered_addresses_reverse_cleanly() {
    let (_, layout) = duo_fixture();

    for fader in 0..layout.fader_count() {
        let path = layout.fader_path(fader).expect("captured fader path");
        assert_eq!(layout.fader_index_from_path(&path), Some(fader));
    }
    for channel in 0..layout.channel_count() {
        let path = layout.channel_path(channel).expect("captured channel path");
        assert_eq!(layout.channel_index_from_path(&path), Some(channel));
    }
    for source in 0..layout.source_count() {
        let path = layout
            .input_source_path(source)
            .expect("captured input-source path");
        assert_eq!(layout.input_source_index_from_path(&path), Some(source));
        for mix in 0..layout.mix_count_per_source() {
            let path = layout
                .mix_cell_path(source, mix)
                .expect("captured mix-cell path");
            assert_eq!(layout.mix_cell_from_path(&path), Some((source, mix)));
        }
    }
    for headphone in 0..layout.headphone_count() {
        let path = layout
            .headphone_path(headphone)
            .expect("captured headphone path");
        assert_eq!(layout.headphone_index_from_path(&path), Some(headphone));
    }
    for effects in 0..layout.effects_count() {
        let path = layout.effects_path(effects).expect("captured effects path");
        assert_eq!(layout.effects_index_from_path(&path), Some(effects));
    }
    for pad in 0..layout.pad_count() {
        let path = layout.pad_path(pad).expect("captured pad path");
        assert_eq!(layout.pad_index_from_path(&path), Some(pad));
    }
}

#[test]
fn duo_174_full_sync_decodes_to_semantic_events() {
    let (packet, _) = Packet::from_bytes(DUO_174).unwrap();
    let (_, layout) = duo_fixture();
    let event = decode_event(&packet.payload, &layout).expect("full sync event");
    let DeviceEvent::InitialState(events) = event else {
        panic!("full sync must produce initial state");
    };

    let unknown = events
        .iter()
        .filter(|event| matches!(event, DeviceEvent::Unknown { .. }))
        .count();
    eprintln!(
        "Duo 1.7.4 initial events: {}, unknown: {unknown}",
        events.len()
    );

    // This baseline is intentionally exact. A decoder change that silently
    // drops a property family must be reviewed instead of shrinking coverage.
    assert_eq!(events.len(), 3_068);
    assert_eq!(
        unknown, 0,
        "all surfaced properties in this capture are typed"
    );
    assert!(events
        .iter()
        .any(|event| matches!(event, DeviceEvent::SystemParamChanged { .. })));
    assert!(events
        .iter()
        .any(|event| matches!(event, DeviceEvent::FaderMuteChanged { .. })));
    assert!(events
        .iter()
        .any(|event| matches!(event, DeviceEvent::MixLevelChanged { .. })));
}

#[test]
fn duo_fixture_rejects_every_truncated_transport_frame() {
    for len in 0..DUO_174.len() {
        assert!(
            Packet::from_bytes(&DUO_174[..len]).is_none(),
            "accepted transport frame truncated at byte {len}"
        );
    }
}

#[test]
fn duo_fixture_rejects_truncated_and_overlong_change_frames() {
    let (packet, _) = Packet::from_bytes(DUO_174).unwrap();
    let payload = &packet.payload;

    let mut cut_points = (0..payload.len()).step_by(4_096).collect::<Vec<_>>();
    cut_points.extend(payload.len().saturating_sub(256)..payload.len());
    cut_points.sort_unstable();
    cut_points.dedup();
    for len in cut_points {
        assert!(
            decode(&payload[..len]).is_none(),
            "accepted full-sync payload truncated at byte {len}"
        );
    }

    let mut overlong = payload.clone();
    overlong.push(0);
    assert!(
        decode(&overlong).is_none(),
        "accepted trailing payload byte"
    );
}

#[test]
fn duo_fixture_contains_only_sanitized_values() {
    let (root, _) = duo_fixture();
    assert_sanitized(&root);
}

#[test]
fn sanitized_duo_fixture_is_codec_stable() {
    let (packet, _) = Packet::from_bytes(DUO_174).unwrap();
    let ChangeFrame::FullSync { root } = decode(&packet.payload).unwrap() else {
        unreachable!();
    };
    let encoded = Packet::new(encode_full_sync(&root)).to_bytes();
    assert_eq!(encoded, DUO_174);
}

fn assert_sanitized(node: &rodecaster_protocol::Node) {
    for property in &node.properties {
        match &property.value {
            Value::Bool(false) | Value::Int64(0) | Value::Double(0.0) | Value::Undefined => {}
            Value::Int(1) if property.name == "boardType" => {}
            Value::Int(0) => {}
            Value::String(value) if property.name == "systemFirmwareVersion" => {
                assert_eq!(value, "1.7.4");
            }
            Value::String(value) if property.name == "mixLevelWithAnchor" => {
                assert_eq!(value, "0.0|0.0");
            }
            Value::String(value) => assert_eq!(value, "<redacted>"),
            Value::Array(values) => assert!(values_are_sanitized(values)),
            Value::Binary(data) | Value::Unknown { data, .. } => {
                assert!(data.iter().all(|byte| *byte == 0));
            }
            other => panic!("unsanitized {} value: {other:?}", property.name),
        }
    }
    for child in &node.children {
        assert_sanitized(child);
    }
}

fn values_are_sanitized(values: &[Value]) -> bool {
    values.iter().all(|value| match value {
        Value::Bool(false)
        | Value::Int(0)
        | Value::Int64(0)
        | Value::Double(0.0)
        | Value::Undefined => true,
        Value::String(value) => value == "<redacted>",
        Value::Array(nested) => values_are_sanitized(nested),
        Value::Binary(data) | Value::Unknown { data, .. } => data.iter().all(|byte| *byte == 0),
        _ => false,
    })
}

fn find_property<'a>(node: &'a rodecaster_protocol::Node, name: &str) -> Option<&'a Value> {
    node.properties
        .iter()
        .find(|property| property.name == name)
        .map(|property| &property.value)
        .or_else(|| {
            node.children
                .iter()
                .find_map(|child| find_property(child, name))
        })
}

fn tree_size(node: &rodecaster_protocol::Node) -> (usize, usize) {
    node.children
        .iter()
        .fold((1, node.properties.len()), |(nodes, properties), child| {
            let (child_nodes, child_properties) = tree_size(child);
            (nodes + child_nodes, properties + child_properties)
        })
}
