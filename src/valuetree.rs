//! JUCE `ValueTree` data structures and parser.
//!
//! When a RØDECaster connects, it transmits its entire device state as a
//! binary `juce::ValueTreeSynchroniser` full-sync payload. This module parses
//! that binary representation into a hierarchy of [`Node`] and [`Property`]
//! structures.

use std::fmt;

use crate::juce_var::{write_compressed_int, Reader, Value};

// Property and Node

/// A property (name-value pair) in the ValueTree.
#[derive(Debug, Clone, PartialEq)]
pub struct Property {
    pub name: String,
    pub value: Value,
}

/// A node in the ValueTree (has a name, properties, and child nodes).
#[derive(Debug, Clone, PartialEq)]
pub struct Node {
    pub name: String,
    pub properties: Vec<Property>,
    pub children: Vec<Node>,
}

impl Node {
    /// Encode this node using JUCE's `ValueTree::writeToStream` format.
    ///
    /// This writes the tree body only. Use
    /// [`crate::change_frame::encode_full_sync`] to prepend the full-sync
    /// change type, then wrap that payload in a transport packet if needed.
    pub fn write_to_stream(&self, out: &mut Vec<u8>) {
        out.extend_from_slice(self.name.as_bytes());
        out.push(0);
        write_compressed_int(out, self.properties.len() as i64);
        for property in &self.properties {
            out.extend_from_slice(property.name.as_bytes());
            out.push(0);
            property.value.write_to_stream(out);
        }
        write_compressed_int(out, self.children.len() as i64);
        for child in &self.children {
            child.write_to_stream(out);
        }
    }
}

impl fmt::Display for Node {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Node({}, {} props, {} children)",
            self.name,
            self.properties.len(),
            self.children.len()
        )
    }
}

// Tree walk: ValueTree::writeToStream framing over juce_var values.

/// Parse a property: `writeString(name)` then a `juce::var` value.
fn parse_property(reader: &mut Reader) -> Option<Property> {
    let name = reader.read_cstring()?.to_string();
    let value = reader.read_value()?;
    Some(Property { name, value })
}

/// Parse one `juce::ValueTree::writeToStream` node from the cursor: name,
/// property count, properties, child count, children (recursive).
///
/// Exposed at crate scope so [`crate::change_frame`] can decode the embedded
/// ValueTree streams in `FullSync` (after the change-type byte) and
/// `ChildAdded` (after the path + insertion index) without re-implementing the
/// tree walk.
pub(crate) fn parse_node(reader: &mut Reader) -> Option<Node> {
    // Node name (writeString: UTF-8 + NUL).
    let name = reader.read_cstring()?.to_string();

    // Property count (writeCompressedInt).
    let prop_count = usize::try_from(reader.read_compressed_int()?).ok()?;
    let mut properties = Vec::with_capacity(prop_count.min(reader.remaining()));
    for _ in 0..prop_count {
        properties.push(parse_property(reader)?);
    }

    // Child count (writeCompressedInt), then children recursively. A declared
    // count is a hard contract: returning a partial tree on truncation would
    // let a corrupted full sync look valid and build a subtly incomplete
    // Layout.
    let child_count = usize::try_from(reader.read_compressed_int()?).ok()?;
    let mut children = Vec::with_capacity(child_count.min(reader.remaining()));
    for _ in 0..child_count {
        children.push(parse_node(reader)?);
    }

    Some(Node {
        name,
        properties,
        children,
    })
}

/// Parse a ValueTree from binary data.
///
/// The data is the payload after the RODE transport frame, i.e. the
/// `juce::ValueTreeSynchroniser` message: a one-byte `ChangeType` header
/// (`fullSync = 2`) followed by `ValueTree::writeToStream` of the root node.
pub fn parse_valuetree(data: &[u8]) -> Option<Node> {
    if data.is_empty() {
        return None;
    }

    let mut reader = Reader::new(data);

    // ChangeType::fullSync header byte (juce::ValueTreeSynchroniser).
    let change_type = reader.read_u8()?;
    if change_type != 0x02 {
        return None;
    }

    // Parse exactly one root node. Trailing bytes are malformed at this layer;
    // transport framing is responsible for separating adjacent messages.
    let root = parse_node(&mut reader)?;
    (reader.remaining() == 0).then_some(root)
}

// Tests

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_property() {
        // faderLevel = 75: "faderLevel\0" + frame [01][05][01][4b 00 00 00].
        let data = [
            0x66, 0x61, 0x64, 0x65, 0x72, 0x4c, 0x65, 0x76, 0x65, 0x6c,
            0x00, // "faderLevel\0"
            0x01, // data_len_len
            0x05, // data_len
            0x01, // type (int)
            0x4b, 0x00, 0x00, 0x00, // value = 75
        ];

        let mut reader = Reader::new(&data);
        let prop = parse_property(&mut reader).expect("Should parse property");
        assert_eq!(prop.name, "faderLevel");
        assert_eq!(prop.value, Value::Int(75));
    }

    #[test]
    fn test_parse_boolean_true() {
        let data = [
            0x6d, 0x65, 0x74, 0x65, 0x72, 0x53, 0x74, 0x65, 0x72, 0x65, 0x6f,
            0x00, // "meterStereo\0"
            0x01, 0x01, 0x02, // var bool true
        ];

        let mut reader = Reader::new(&data);
        let prop = parse_property(&mut reader).expect("Should parse property");
        assert_eq!(prop.name, "meterStereo");
        assert_eq!(prop.value, Value::Bool(true));
    }

    #[test]
    fn test_parse_double() {
        // meterPeakL = 1.0 (0x3FF0000000000000 LE).
        let data = [
            0x6d, 0x65, 0x74, 0x65, 0x72, 0x50, 0x65, 0x61, 0x6b, 0x4c,
            0x00, // "meterPeakL\0"
            0x01, 0x09, 0x04, // var double frame
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xf0, 0x3f, // 1.0
        ];

        let mut reader = Reader::new(&data);
        let prop = parse_property(&mut reader).expect("Should parse property");
        assert_eq!(prop.name, "meterPeakL");
        if let Value::Double(d) = prop.value {
            assert!((d - 1.0).abs() < 0.0001);
        } else {
            panic!("Expected Double value");
        }
    }

    #[test]
    fn test_parse_binary_property() {
        // mixUnlinkRequest: the real 6-byte binary blob protocol::mix emits.
        let mut data = b"mixUnlinkRequest\0".to_vec();
        data.extend_from_slice(&[0x01, 0x07, 0x08, 0x01, 0x01, 0x02, 0x01, 0x01, 0x02]);

        let mut reader = Reader::new(&data);
        let prop = parse_property(&mut reader).expect("Should parse property");
        assert_eq!(prop.name, "mixUnlinkRequest");
        assert_eq!(
            prop.value,
            Value::Binary(vec![0x01, 0x01, 0x02, 0x01, 0x01, 0x02])
        );
    }

    #[test]
    fn node_stream_round_trips() {
        let node = Node {
            name: "ROOT".to_string(),
            properties: vec![Property {
                name: "name".to_string(),
                value: Value::String("fixture".to_string()),
            }],
            children: vec![Node {
                name: "CHILD".to_string(),
                properties: vec![Property {
                    name: "enabled".to_string(),
                    value: Value::Bool(true),
                }],
                children: vec![],
            }],
        };
        let mut bytes = Vec::new();
        node.write_to_stream(&mut bytes);
        let mut reader = Reader::new(&bytes);
        assert_eq!(parse_node(&mut reader), Some(node));
        assert_eq!(reader.remaining(), 0);
    }

    #[test]
    fn test_node_display() {
        let node = Node {
            name: "FADER".to_string(),
            properties: vec![
                Property {
                    name: "faderLevel".to_string(),
                    value: Value::Int(75),
                },
                Property {
                    name: "channelOutputMute".to_string(),
                    value: Value::Bool(false),
                },
            ],
            children: vec![],
        };
        assert_eq!(node.to_string(), "Node(FADER, 2 props, 0 children)");
    }
}
