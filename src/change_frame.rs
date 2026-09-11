//! JUCE `ValueTreeSynchroniser` change-frame encoder and decoder.
//!
//! Handles low-level incremental change messages such as property modifications
//! and child node additions or removals.

use crate::juce_var::{write_compressed_int, Reader, Value};
use crate::valuetree::{parse_node, Node};

const PROPERTY_CHANGED: u8 = 1;
const FULL_SYNC: u8 = 2;
const CHILD_ADDED: u8 = 3;
const CHILD_REMOVED: u8 = 4;
const CHILD_MOVED: u8 = 5;
const PROPERTY_REMOVED: u8 = 6;

/// A decoded ValueTree change frame.
#[derive(Debug, Clone, PartialEq)]
pub enum ChangeFrame {
    PropertyChanged {
        path: Vec<u32>,
        name: String,
        value: Value,
    },
    PropertyRemoved {
        path: Vec<u32>,
        name: String,
    },
    ChildAdded {
        path: Vec<u32>,
        index: u32,
        subtree: Node,
    },
    ChildRemoved {
        path: Vec<u32>,
        old_index: u32,
    },
    ChildMoved {
        path: Vec<u32>,
        old_index: u32,
        new_index: u32,
    },
    FullSync {
        root: Node,
    },
}

impl ChangeFrame {
    /// Path component of the change, if any (empty for `FullSync`).
    pub fn path(&self) -> &[u32] {
        match self {
            ChangeFrame::PropertyChanged { path, .. }
            | ChangeFrame::PropertyRemoved { path, .. }
            | ChangeFrame::ChildAdded { path, .. }
            | ChangeFrame::ChildRemoved { path, .. }
            | ChangeFrame::ChildMoved { path, .. } => path,
            ChangeFrame::FullSync { .. } => &[],
        }
    }

    /// True if the change alters tree topology.
    pub fn is_structural(&self) -> bool {
        matches!(
            self,
            ChangeFrame::ChildAdded { .. }
                | ChangeFrame::ChildRemoved { .. }
                | ChangeFrame::ChildMoved { .. }
                | ChangeFrame::FullSync { .. }
        )
    }
}

/// Decode a change-frame payload.
pub fn decode(payload: &[u8]) -> Option<ChangeFrame> {
    let mut reader = Reader::new(payload);
    let change_type = reader.read_u8()?;
    let frame = match change_type {
        PROPERTY_CHANGED => {
            let path = read_path(&mut reader)?;
            let name = reader.read_cstring()?.to_string();
            let value = reader.read_value()?;
            ChangeFrame::PropertyChanged { path, name, value }
        }
        FULL_SYNC => {
            let root = parse_node(&mut reader)?;
            ChangeFrame::FullSync { root }
        }
        CHILD_ADDED => {
            let path = read_path(&mut reader)?;
            let index = read_compint_u32(&mut reader)?;
            let subtree = parse_node(&mut reader)?;
            ChangeFrame::ChildAdded {
                path,
                index,
                subtree,
            }
        }
        CHILD_REMOVED => {
            let path = read_path(&mut reader)?;
            let old_index = read_compint_u32(&mut reader)?;
            ChangeFrame::ChildRemoved { path, old_index }
        }
        CHILD_MOVED => {
            let path = read_path(&mut reader)?;
            let old_index = read_compint_u32(&mut reader)?;
            let new_index = read_compint_u32(&mut reader)?;
            ChangeFrame::ChildMoved {
                path,
                old_index,
                new_index,
            }
        }
        PROPERTY_REMOVED => {
            let path = read_path(&mut reader)?;
            let name = reader.read_cstring()?.to_string();
            ChangeFrame::PropertyRemoved { path, name }
        }
        _ => return None,
    };
    (reader.remaining() == 0).then_some(frame)
}

/// Encode a complete `fullSync` change-frame payload.
pub fn encode_full_sync(root: &Node) -> Vec<u8> {
    let mut out = Vec::new();
    out.push(FULL_SYNC);
    root.write_to_stream(&mut out);
    out
}

/// Encode a `propertyChanged` frame.
pub fn encode_property_changed(path: &[u32], prop_name: &str, value: &Value) -> Vec<u8> {
    let mut out = Vec::with_capacity(8 + prop_name.len() + 16);
    out.push(PROPERTY_CHANGED);
    write_compressed_int(&mut out, path.len() as i64);
    for &p in path {
        write_compressed_int(&mut out, p as i64);
    }
    out.extend_from_slice(prop_name.as_bytes());
    out.push(0);
    value.write_to_stream(&mut out);
    out
}

fn read_path(reader: &mut Reader) -> Option<Vec<u32>> {
    let n = reader.read_compressed_int()?;
    if n < 0 {
        return None;
    }
    let n = n as usize;
    let cap = n.min(reader.remaining());
    let mut path = Vec::with_capacity(cap);
    for _ in 0..n {
        path.push(read_compint_u32(reader)?);
    }
    Some(path)
}

fn read_compint_u32(reader: &mut Reader) -> Option<u32> {
    let v = reader.read_compressed_int()?;
    if v < 0 {
        return None;
    }
    u32::try_from(v).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::valuetree::Property;

    /// PropertyChanged for `mixLevelWithAnchor` with path = [217] (single-level).
    /// This is the exact vector the existing parser test uses, rebuilt as a
    /// change-frame payload (`[01]` + path + `[name\0]` + var).
    #[test]
    fn decodes_property_changed_mix_level() {
        let bytes = [
            0x01, // changeType propertyChanged
            0x01, 0x01, // writeCompressedInt(1) -> nLevels = 1
            0x01, 0xd9, // writeCompressedInt(217) -> path[0] = 217
            b'm', b'i', b'x', b'L', b'e', b'v', b'e', b'l', b'W', b'i', b't', b'h', b'A', b'n',
            b'c', b'h', b'o', b'r', 0x00, // property name
            0x01, 0x13, 0x05, b'0', b'.', b'4', b'7', b'2', b'4', b'4', b'1', b'|', b'0', b'.',
            b'4', b'7', b'2', b'4', b'4', b'1', 0x00, // var String
        ];

        let frame = decode(&bytes).expect("decodes");
        match frame {
            ChangeFrame::PropertyChanged { path, name, value } => {
                assert_eq!(path, vec![217]);
                assert_eq!(name, "mixLevelWithAnchor");
                assert_eq!(value, Value::String("0.472441|0.472441".to_string()));
            }
            other => panic!("expected PropertyChanged, got {other:?}"),
        }
    }

    /// PropertyChanged for `faderLevel` with path = [0, 4+f] (two-level), the
    /// exact preamble the existing FaderLevel encoder emits.
    #[test]
    fn decodes_property_changed_fader_level_two_level_path() {
        // [01] type + writeCompressedInt(2) + writeCompressedInt(0) + writeCompressedInt(4+3)
        // + "faderLevel\0" + var Int 75
        let bytes = [
            0x01, // propertyChanged
            0x01, 0x02, // nLevels = 2
            0x00, // path[0] = 0  (writeCompressedInt(0) is a single zero byte)
            0x01, 0x07, // path[1] = 7  (4 + fader=3)
            b'f', b'a', b'd', b'e', b'r', b'L', b'e', b'v', b'e', b'l', 0x00, // property name
            0x01, 0x05, 0x01, 0x4b, 0x00, 0x00, 0x00, // var Int 75
        ];

        let frame = decode(&bytes).expect("decodes");
        match frame {
            ChangeFrame::PropertyChanged { path, name, value } => {
                assert_eq!(path, vec![0, 7]);
                assert_eq!(name, "faderLevel");
                assert_eq!(value, Value::Int(75));
            }
            other => panic!("expected PropertyChanged, got {other:?}"),
        }
    }

    #[test]
    fn round_trips_property_changed_single_level() {
        let bytes = encode_property_changed(&[217], "mixLevelWithAnchor", &Value::Bool(true));
        let frame = decode(&bytes).expect("decodes");
        assert_eq!(
            frame,
            ChangeFrame::PropertyChanged {
                path: vec![217],
                name: "mixLevelWithAnchor".to_string(),
                value: Value::Bool(true),
            }
        );
    }

    #[test]
    fn round_trips_property_changed_two_level() {
        let bytes = encode_property_changed(&[0, 7], "faderLevel", &Value::Int(75));
        let frame = decode(&bytes).expect("decodes");
        assert_eq!(
            frame,
            ChangeFrame::PropertyChanged {
                path: vec![0, 7],
                name: "faderLevel".to_string(),
                value: Value::Int(75),
            }
        );
    }

    #[test]
    fn round_trips_property_changed_empty_path() {
        // Change at root has no path entries.
        let bytes = encode_property_changed(&[], "rootProp", &Value::Bool(false));
        let frame = decode(&bytes).expect("decodes");
        assert_eq!(
            frame,
            ChangeFrame::PropertyChanged {
                path: vec![],
                name: "rootProp".to_string(),
                value: Value::Bool(false),
            }
        );
    }

    #[test]
    fn full_sync_round_trips_via_encoder() {
        let root = Node {
            name: "ROOT".to_string(),
            properties: vec![],
            children: vec![Node {
                name: "CHILD".to_string(),
                properties: vec![],
                children: vec![],
            }],
        };
        let payload = encode_full_sync(&root);
        assert_eq!(decode(&payload), Some(ChangeFrame::FullSync { root }));
    }

    #[test]
    fn decodes_full_sync_minimal() {
        // [02] + ValueTree stream of <ROOT/> (name + 0 props + 0 children).
        let bytes = [
            0x02, // fullSync
            b'R', b'O', b'O', b'T', 0x00, // node name "ROOT\0"
            0x00, // property count = 0 (writeCompressedInt(0))
            0x00, // child count = 0
        ];

        let frame = decode(&bytes).expect("decodes");
        match frame {
            ChangeFrame::FullSync { root } => {
                assert_eq!(root.name, "ROOT");
                assert!(root.properties.is_empty());
                assert!(root.children.is_empty());
            }
            other => panic!("expected FullSync, got {other:?}"),
        }
    }

    #[test]
    fn decodes_property_removed() {
        let bytes = [
            0x06, // propertyRemoved
            0x01, 0x01, 0x01, 0x05, // path = [5]
            b'g', b'o', b'n', b'e', 0x00,
        ];
        let frame = decode(&bytes).expect("decodes");
        assert_eq!(
            frame,
            ChangeFrame::PropertyRemoved {
                path: vec![5],
                name: "gone".to_string(),
            }
        );
    }

    #[test]
    fn decodes_child_removed() {
        let bytes = [
            0x04, // childRemoved
            0x01, 0x01, 0x01, 0x02, // path = [2]
            0x01, 0x03, // oldIndex = 3
        ];
        let frame = decode(&bytes).expect("decodes");
        assert_eq!(
            frame,
            ChangeFrame::ChildRemoved {
                path: vec![2],
                old_index: 3,
            }
        );
    }

    #[test]
    fn decodes_child_moved() {
        let bytes = [
            0x05, // childMoved
            0x01, 0x01, 0x01, 0x02, // path = [2]
            0x01, 0x01, // oldIndex = 1
            0x01, 0x04, // newIndex = 4
        ];
        let frame = decode(&bytes).expect("decodes");
        assert_eq!(
            frame,
            ChangeFrame::ChildMoved {
                path: vec![2],
                old_index: 1,
                new_index: 4,
            }
        );
    }

    #[test]
    fn decodes_child_added_with_subtree() {
        let bytes = [
            0x03, // childAdded
            0x01, 0x01, 0x01, 0x02, // path = [2]
            0x01, 0x00, // index = 0
            // ValueTree stream of <NEW prop=Int(7)/>
            b'N', b'E', b'W', 0x00, // name
            0x01, 0x01, // property count = 1
            b'p', b'r', b'o', b'p', 0x00, // property name
            0x01, 0x05, 0x01, 0x07, 0x00, 0x00, 0x00, // var Int 7
            0x00, // child count = 0
        ];
        let frame = decode(&bytes).expect("decodes");
        match frame {
            ChangeFrame::ChildAdded {
                path,
                index,
                subtree,
            } => {
                assert_eq!(path, vec![2]);
                assert_eq!(index, 0);
                assert_eq!(subtree.name, "NEW");
                assert_eq!(
                    subtree.properties,
                    vec![Property {
                        name: "prop".to_string(),
                        value: Value::Int(7),
                    }]
                );
                assert!(subtree.children.is_empty());
            }
            other => panic!("expected ChildAdded, got {other:?}"),
        }
    }

    #[test]
    fn structural_flag_matches_expectations() {
        let pc = ChangeFrame::PropertyChanged {
            path: vec![],
            name: String::new(),
            value: Value::Undefined,
        };
        let pr = ChangeFrame::PropertyRemoved {
            path: vec![],
            name: String::new(),
        };
        let ca = ChangeFrame::ChildAdded {
            path: vec![],
            index: 0,
            subtree: Node {
                name: String::new(),
                properties: vec![],
                children: vec![],
            },
        };
        let cr = ChangeFrame::ChildRemoved {
            path: vec![],
            old_index: 0,
        };
        let cm = ChangeFrame::ChildMoved {
            path: vec![],
            old_index: 0,
            new_index: 0,
        };
        let fs = ChangeFrame::FullSync {
            root: Node {
                name: String::new(),
                properties: vec![],
                children: vec![],
            },
        };

        assert!(!pc.is_structural());
        assert!(!pr.is_structural());
        assert!(ca.is_structural());
        assert!(cr.is_structural());
        assert!(cm.is_structural());
        assert!(fs.is_structural());
    }

    #[test]
    fn rejects_empty_payload() {
        assert_eq!(decode(&[]), None);
    }

    #[test]
    fn rejects_unknown_change_type() {
        assert_eq!(decode(&[0xff]), None);
    }
}
