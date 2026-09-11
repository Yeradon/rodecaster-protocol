//! Shared test fixtures and layout builders for unit tests.

use crate::layout::Layout;
use crate::valuetree::{Node, Property};
use crate::Value;

pub fn n(name: &str) -> Node {
    Node {
        name: name.to_string(),
        properties: vec![],
        children: vec![],
    }
}

pub fn np(name: &str, properties: Vec<Property>) -> Node {
    Node {
        name: name.to_string(),
        properties,
        children: vec![],
    }
}

pub fn nc(name: &str, children: Vec<Node>) -> Node {
    Node {
        name: name.to_string(),
        properties: vec![],
        children,
    }
}

pub fn prop(name: &str, value: Value) -> Property {
    Property {
        name: name.to_string(),
        value,
    }
}

/// The canonical synthetic root node used across command and event unit tests.
pub fn synthetic_root() -> Node {
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
    // 19 INPUTSOURCE nodes (addressable source run), after MIX.
    for _ in 0..19 {
        children.push(n("INPUTSOURCE"));
    }
    // Singleton families (one each), after the INPUTSOURCE run.
    children.push(n("MASTERCHANNEL"));
    children.push(n("OUTPUT"));
    children.push(n("DUCKER"));
    children.push(n("RECORDER"));
    children.push(n("PLAYER"));
    // HEADPHONE is multi-instance; two at the tail (run length 2).
    children.push(n("HEADPHONE"));
    children.push(n("HEADPHONE"));
    // EFFECTS_PARAMETERS is multi-instance; three at the tail (run length 3).
    children.push(n("EFFECTS_PARAMETERS"));
    children.push(n("EFFECTS_PARAMETERS"));
    children.push(n("EFFECTS_PARAMETERS"));
    // GUI is the single front-panel UI-state node (singleton) at the tail.
    children.push(n("GUI"));
    // SOUNDPADS container with a run of 3 PAD nodes (a non-PAD child first so
    // first_pad is not zero, mirroring the real tree).
    children.push(nc(
        "SOUNDPADS",
        vec![n("PADHEADER"), n("PAD"), n("PAD"), n("PAD")],
    ));
    // SYSTEM is the single device-wide state node (singleton) at the tail.
    children.push(n("SYSTEM"));
    nc("DEVICE", children)
}

/// Build the standard layout from `synthetic_root()`.
pub fn layout() -> Layout {
    Layout::from_full_sync(&synthetic_root()).unwrap()
}

/// Minimal layout containing only the required base nodes.
pub fn minimal_layout() -> Layout {
    let phys = nc("PHYSICALINTERFACE", vec![n("FADER")]);
    let mut children = vec![phys, n("CHANNEL")];
    for _ in 0..13 {
        children.push(n("MIX"));
    }
    Layout::from_full_sync(&nc("DEVICE", children)).unwrap()
}
