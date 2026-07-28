//! Sanitize a captured TCP full-sync frame for safe use as a test fixture.
//!
//! Every property value is replaced with a type-preserving neutral value,
//! except the non-sensitive model discriminator and firmware version. Node and
//! property names, ordering, and tree topology remain unchanged.
//!
//! Usage:
//! `cargo run --example sanitize_full_sync -- raw-frame.bin sanitized-frame.bin`

use std::env;
use std::fs;

use rodecaster_protocol::change_frame::{decode, encode_full_sync, ChangeFrame};
use rodecaster_protocol::frame::Packet;
use rodecaster_protocol::{Layout, Node, Value};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = env::args_os().skip(1);
    let input = args.next().ok_or("missing input frame path")?;
    let output = args.next().ok_or("missing output frame path")?;
    if args.next().is_some() {
        return Err("expected exactly two paths: input and output".into());
    }

    let bytes = fs::read(&input)?;
    let (packet, consumed) =
        Packet::from_bytes(&bytes).ok_or("input is not a complete TCP frame")?;
    if consumed != bytes.len() {
        return Err(format!("input contains {} trailing bytes", bytes.len() - consumed).into());
    }
    let ChangeFrame::FullSync { mut root } =
        decode(&packet.payload).ok_or("invalid change frame")?
    else {
        return Err("input frame is not a full sync".into());
    };

    let layout = Layout::from_full_sync(&root)?;
    let firmware = find_property(&root, "systemFirmwareVersion")
        .and_then(|value| match value {
            Value::String(value) => Some(value.as_str()),
            _ => None,
        })
        .unwrap_or("unknown")
        .to_string();
    let (nodes, properties) = tree_size(&root);

    sanitize_node(&mut root);
    let sanitized = Packet::new(encode_full_sync(&root)).to_bytes();
    fs::write(&output, &sanitized)?;

    eprintln!("model={}", layout.model());
    eprintln!("firmware={firmware}");
    eprintln!("faders={}", layout.fader_count());
    eprintln!("channels={}", layout.channel_count());
    eprintln!("sources={}", layout.source_count());
    eprintln!("mixes_per_source={}", layout.mix_count_per_source());
    eprintln!("input_sources={}", layout.input_source_count());
    eprintln!("headphones={}", layout.headphone_count());
    eprintln!("effects={}", layout.effects_count());
    eprintln!("pads={}", layout.pad_count());
    eprintln!("sip_call_slots={}", layout.sip_call_slots_count());
    eprintln!("sip_registrations={}", layout.sip_registration_count());
    eprintln!("nodes={nodes}");
    eprintln!("properties={properties}");
    eprintln!("frame_bytes={}", sanitized.len());
    Ok(())
}

fn sanitize_node(node: &mut Node) {
    for property in &mut node.properties {
        property.value = sanitize_value(&property.name, &property.value);
    }
    for child in &mut node.children {
        sanitize_node(child);
    }
}

fn sanitize_value(name: &str, value: &Value) -> Value {
    match value {
        Value::Bool(_) => Value::Bool(false),
        Value::Int(value) if name == "boardType" => Value::Int(*value),
        Value::Int(_) => Value::Int(0),
        Value::Int64(_) => Value::Int64(0),
        Value::Double(_) => Value::Double(0.0),
        Value::String(value) if name == "systemFirmwareVersion" => Value::String(value.clone()),
        Value::String(_) if name == "mixLevelWithAnchor" => Value::String("0.0|0.0".into()),
        Value::String(_) => Value::String("<redacted>".into()),
        Value::Array(values) => Value::Array(
            values
                .iter()
                .map(|value| sanitize_value(name, value))
                .collect(),
        ),
        Value::Binary(data) => Value::Binary(vec![0; data.len()]),
        Value::Undefined => Value::Undefined,
        Value::Unknown { type_id, data } => Value::Unknown {
            type_id: *type_id,
            data: vec![0; data.len()],
        },
    }
}

fn find_property<'a>(node: &'a Node, name: &str) -> Option<&'a Value> {
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

fn tree_size(node: &Node) -> (usize, usize) {
    node.children
        .iter()
        .fold((1, node.properties.len()), |(nodes, properties), child| {
            let (child_nodes, child_properties) = tree_size(child);
            (nodes + child_nodes, properties + child_properties)
        })
}
