//! Runtime device-layout discovery from a parsed fullSync.
//!
//! Where the addressable node families (`PHYSICALINTERFACE`, `FADER`,
//! `CHANNEL`, `MIX`) sit inside the fullSync tree is **per-device,
//! per-firmware**. The crate must not hardcode positions. [`Layout`] walks the
//! parsed tree by node name at connect time and records the discovered
//! positions; encoders and decoders translate logical addresses (fader index,
//! mix cell) through it.
//!
//! Build once per connection from a fullSync; replace the whole value on
//! resync. Plain immutable data, `Send + Sync` automatic.

use crate::juce_var::Value;
use crate::names::DeviceModel;
use crate::valuetree::Node;

/// Mix destinations per source in the RODECaster Pro II / Duo mix matrix
/// (firmware 1.7.3 observation).
///
/// This is a **device characteristic**, not a JUCE protocol fact. The total
/// `MIX` node count factors as `source_count * mix_count_per_source`; one
/// dimension cannot be uniquely recovered from the fullSync alone, so this
/// constant pins it. If newer firmware changes the matrix width, expose an
/// override here.
pub const MIX_COUNT_PER_SOURCE: u8 = 13;

/// Tree positions of the addressable node families. All fields are
/// discovered, not hardcoded.
#[derive(Debug, Clone, PartialEq)]
pub struct Layout {
    model: DeviceModel,
    physical_interface_idx: u32,
    first_fader_in_phys: u32,
    fader_count: u8,
    first_channel: u32,
    channel_count: u8,
    first_mix: u32,
    source_count: u8,
}

impl Layout {
    /// Walk a parsed fullSync root and discover the layout.
    ///
    /// Fails loudly on missing required nodes so a future firmware layout
    /// change surfaces immediately rather than silently misrouting.
    pub fn from_full_sync(root: &Node) -> Result<Layout, BuildError> {
        // Device model from the SYSTEM node (boardType primary, systemName
        // fallback). Absent on synthetic trees and old firmware -> Pro II.
        let model = detect_model(root);

        // PHYSICALINTERFACE under root, with FADER children inside it.
        let physical_interface_idx = position_of_named_child(root, "PHYSICALINTERFACE")
            .ok_or(BuildError::MissingNode("PHYSICALINTERFACE under root"))?;
        let phys = &root.children[physical_interface_idx as usize];

        let first_fader_in_phys = position_of_named_child(phys, "FADER")
            .ok_or(BuildError::MissingNode("FADER under PHYSICALINTERFACE"))?;
        let fader_count = count_consecutive_named(phys, first_fader_in_phys, "FADER");

        // CHANNEL nodes under root.
        let first_channel = position_of_named_child(root, "CHANNEL")
            .ok_or(BuildError::MissingNode("CHANNEL under root"))?;
        let channel_count = count_consecutive_named(root, first_channel, "CHANNEL");

        // MIX nodes under root. Count, then factor by MIX_COUNT_PER_SOURCE.
        let first_mix = position_of_named_child(root, "MIX")
            .ok_or(BuildError::MissingNode("MIX under root"))?;
        let mix_total = count_consecutive_named_u32(root, first_mix, "MIX");
        if mix_total == 0 || !mix_total.is_multiple_of(MIX_COUNT_PER_SOURCE as u32) {
            return Err(BuildError::ShapeMismatch {
                what: "MIX",
                detail: format!(
                    "expected mix_total divisible by MIX_COUNT_PER_SOURCE={}, got {}",
                    MIX_COUNT_PER_SOURCE, mix_total
                ),
            });
        }
        let source_count = u8::try_from(mix_total / MIX_COUNT_PER_SOURCE as u32).map_err(|_| {
            BuildError::ShapeMismatch {
                what: "MIX",
                detail: format!("source_count overflows u8 (mix_total={mix_total})"),
            }
        })?;

        Ok(Layout {
            model,
            physical_interface_idx,
            first_fader_in_phys,
            fader_count,
            first_channel,
            channel_count,
            first_mix,
            source_count,
        })
    }

    /// Which RODECaster this layout was discovered from. Selects the per-model
    /// [`crate::Fader`] mapping when resolving typed commands and events.
    pub fn model(&self) -> DeviceModel {
        self.model
    }

    pub fn physical_interface_idx(&self) -> u32 {
        self.physical_interface_idx
    }
    pub fn first_fader_in_phys(&self) -> u32 {
        self.first_fader_in_phys
    }
    pub fn fader_count(&self) -> u8 {
        self.fader_count
    }
    pub fn first_channel(&self) -> u32 {
        self.first_channel
    }
    pub fn channel_count(&self) -> u8 {
        self.channel_count
    }
    pub fn first_mix(&self) -> u32 {
        self.first_mix
    }
    pub fn source_count(&self) -> u8 {
        self.source_count
    }
    pub fn mix_count_per_source(&self) -> u8 {
        MIX_COUNT_PER_SOURCE
    }

    /// Root-down path to the `n`th `CHANNEL` node (single-level).
    /// Used for `channelOutputMute`, `channelCueEnable`, `channelInputSource`.
    pub fn channel_path(&self, n: u8) -> Option<Vec<u32>> {
        if n >= self.channel_count {
            return None;
        }
        Some(vec![self.first_channel + n as u32])
    }

    /// Root-down path to the `n`th `FADER` strip inside `PHYSICALINTERFACE`
    /// (two-level). Used for `faderLevel`.
    pub fn fader_path(&self, n: u8) -> Option<Vec<u32>> {
        if n >= self.fader_count {
            return None;
        }
        Some(vec![
            self.physical_interface_idx,
            self.first_fader_in_phys + n as u32,
        ])
    }

    /// Root-down path to the `MIX` cell at (`source`, `mix`), source-major
    /// layout. Used for `mixLevelWithAnchor`, `mixMute`, `mixDisabled`,
    /// `mixLink`, `mixLinkRequest`, `mixUnlinkRequest`.
    pub fn mix_cell_path(&self, source: u8, mix: u8) -> Option<Vec<u32>> {
        if source >= self.source_count || mix >= MIX_COUNT_PER_SOURCE {
            return None;
        }
        Some(vec![
            self.first_mix + source as u32 * MIX_COUNT_PER_SOURCE as u32 + mix as u32,
        ])
    }

    /// Inverse of `channel_path`: which channel index does this single-level
    /// path identify, if any?
    pub fn channel_index_from_path(&self, path: &[u32]) -> Option<u8> {
        if path.len() != 1 {
            return None;
        }
        let p = path[0];
        if p < self.first_channel {
            return None;
        }
        let n = p - self.first_channel;
        if n >= self.channel_count as u32 {
            return None;
        }
        Some(n as u8)
    }

    /// Inverse of `fader_path`: which fader index does this two-level path
    /// identify, if any?
    pub fn fader_index_from_path(&self, path: &[u32]) -> Option<u8> {
        if path.len() != 2 || path[0] != self.physical_interface_idx {
            return None;
        }
        let p = path[1];
        if p < self.first_fader_in_phys {
            return None;
        }
        let n = p - self.first_fader_in_phys;
        if n >= self.fader_count as u32 {
            return None;
        }
        Some(n as u8)
    }

    /// Inverse of `mix_cell_path`: which (source, mix) cell, if any?
    pub fn mix_cell_from_path(&self, path: &[u32]) -> Option<(u8, u8)> {
        if path.len() != 1 {
            return None;
        }
        let p = path[0];
        if p < self.first_mix {
            return None;
        }
        let offset = p - self.first_mix;
        let span = self.source_count as u32 * MIX_COUNT_PER_SOURCE as u32;
        if offset >= span {
            return None;
        }
        let source = (offset / MIX_COUNT_PER_SOURCE as u32) as u8;
        let mix = (offset % MIX_COUNT_PER_SOURCE as u32) as u8;
        Some((source, mix))
    }
}

/// Read the `SYSTEM` node's `boardType` / `systemName` and resolve the model.
/// Missing `SYSTEM` (synthetic trees, partial captures) -> Pro II default.
fn detect_model(root: &Node) -> DeviceModel {
    let sys = root.children.iter().find(|c| c.name == "SYSTEM");
    let board_type = sys.and_then(|s| {
        s.properties
            .iter()
            .find(|p| p.name == "boardType")
            .and_then(|p| p.value.as_int())
    });
    let system_name = sys.and_then(|s| {
        s.properties
            .iter()
            .find(|p| p.name == "systemName")
            .and_then(|p| match &p.value {
                Value::String(name) => Some(name.as_str()),
                _ => None,
            })
    });
    DeviceModel::detect(board_type, system_name)
}

fn position_of_named_child(parent: &Node, name: &str) -> Option<u32> {
    parent
        .children
        .iter()
        .position(|c| c.name == name)
        .map(|i| i as u32)
}

fn count_consecutive_named(parent: &Node, start: u32, name: &str) -> u8 {
    let mut n: u8 = 0;
    for child in parent.children.iter().skip(start as usize) {
        if child.name != name {
            break;
        }
        n = n.saturating_add(1);
    }
    n
}

fn count_consecutive_named_u32(parent: &Node, start: u32, name: &str) -> u32 {
    let mut n: u32 = 0;
    for child in parent.children.iter().skip(start as usize) {
        if child.name != name {
            break;
        }
        n = n.saturating_add(1);
    }
    n
}

/// Error from [`Layout::from_full_sync`].
#[derive(Debug, Clone, PartialEq)]
pub enum BuildError {
    /// A required node name wasn't found where the Rodecaster layout expects it.
    MissingNode(&'static str),
    /// A required node family was found but its shape doesn't match expectations
    /// (e.g., MIX count not divisible by the matrix width).
    ShapeMismatch { what: &'static str, detail: String },
}

impl std::fmt::Display for BuildError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BuildError::MissingNode(what) => {
                write!(f, "fullSync missing required node: {what}")
            }
            BuildError::ShapeMismatch { what, detail } => {
                write!(f, "fullSync shape mismatch for {what}: {detail}")
            }
        }
    }
}

impl std::error::Error for BuildError {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::valuetree::Property;

    fn node(name: &str) -> Node {
        Node {
            name: name.to_string(),
            properties: vec![],
            children: vec![],
        }
    }
    fn node_with_children(name: &str, children: Vec<Node>) -> Node {
        Node {
            name: name.to_string(),
            properties: vec![],
            children,
        }
    }

    /// Build a synthetic tree where PHYSICALINTERFACE / CHANNEL / MIX are at
    /// **non-default positions** so the test fails if anything is hardcoded
    /// to 0x1C (28), 62, or 0x04. Layout values come from tree positions,
    /// not constants.
    fn synthetic_tree() -> Node {
        let phys = node_with_children(
            "PHYSICALINTERFACE",
            vec![
                node("HEADER"), // index 0
                node("FADER"),  // index 1 (first_fader_in_phys = 1, NOT 4)
                node("FADER"),  // index 2
                node("FADER"),  // index 3
                node("FOOTER"), // breaks FADER sequence
            ],
        );

        // root.children deliberately puts CHANNEL at 4 (NOT 28) and MIX at 8
        // (NOT 62), with 2 sources => mix_total = 26 (= 2 * 13).
        let mut children = vec![
            node("OTHER1"),  // 0
            node("OTHER2"),  // 1
            phys,            // 2: physical_interface_idx = 2 (NOT 0)
            node("OTHER3"),  // 3
            node("CHANNEL"), // 4: first_channel = 4 (NOT 28)
            node("CHANNEL"), // 5
            node("CHANNEL"), // 6
            node("OTHER4"),  // 7: breaks CHANNEL run
        ];
        // first_mix = 8 (NOT 62); 26 MIX nodes -> source_count = 2
        for _ in 0..26 {
            children.push(node("MIX"));
        }
        node_with_children("DEVICE", children)
    }

    #[test]
    fn discovers_bases_from_tree_positions_not_constants() {
        let root = synthetic_tree();
        let layout = Layout::from_full_sync(&root).expect("builds");

        // Critical: these are tree positions, not the 1.7.3-firmware constants.
        assert_eq!(layout.physical_interface_idx(), 2);
        assert_eq!(layout.first_fader_in_phys(), 1);
        assert_eq!(layout.fader_count(), 3);
        assert_eq!(layout.first_channel(), 4);
        assert_eq!(layout.channel_count(), 3);
        assert_eq!(layout.first_mix(), 8);
        assert_eq!(layout.source_count(), 2);
        assert_eq!(layout.mix_count_per_source(), 13);
    }

    #[test]
    fn channel_path_uses_discovered_first_channel() {
        let layout = Layout::from_full_sync(&synthetic_tree()).unwrap();
        // first_channel = 4 in the synthetic tree, so channel 0 lives at [4].
        assert_eq!(layout.channel_path(0), Some(vec![4]));
        assert_eq!(layout.channel_path(2), Some(vec![6]));
        // Out of range.
        assert_eq!(layout.channel_path(3), None);
        // And the inverse.
        assert_eq!(layout.channel_index_from_path(&[4]), Some(0));
        assert_eq!(layout.channel_index_from_path(&[6]), Some(2));
        assert_eq!(layout.channel_index_from_path(&[7]), None);
        assert_eq!(layout.channel_index_from_path(&[3]), None);
        assert_eq!(layout.channel_index_from_path(&[4, 0]), None);
    }

    #[test]
    fn fader_path_is_two_level_through_physical_interface() {
        let layout = Layout::from_full_sync(&synthetic_tree()).unwrap();
        // [physical_interface_idx=2, first_fader_in_phys=1 + n]
        assert_eq!(layout.fader_path(0), Some(vec![2, 1]));
        assert_eq!(layout.fader_path(2), Some(vec![2, 3]));
        assert_eq!(layout.fader_path(3), None);
        // Inverse.
        assert_eq!(layout.fader_index_from_path(&[2, 1]), Some(0));
        assert_eq!(layout.fader_index_from_path(&[2, 3]), Some(2));
        assert_eq!(layout.fader_index_from_path(&[2, 4]), None);
        assert_eq!(layout.fader_index_from_path(&[0, 1]), None); // wrong parent
        assert_eq!(layout.fader_index_from_path(&[1]), None); // wrong length
    }

    #[test]
    fn mix_cell_path_is_source_major() {
        let layout = Layout::from_full_sync(&synthetic_tree()).unwrap();
        // first_mix = 8, 13 mixes per source.
        assert_eq!(layout.mix_cell_path(0, 0), Some(vec![8]));
        assert_eq!(layout.mix_cell_path(0, 12), Some(vec![20]));
        assert_eq!(layout.mix_cell_path(1, 0), Some(vec![21]));
        assert_eq!(layout.mix_cell_path(1, 12), Some(vec![33]));
        assert_eq!(layout.mix_cell_path(2, 0), None); // source out of range
        assert_eq!(layout.mix_cell_path(0, 13), None); // mix out of range
                                                       // Inverse.
        assert_eq!(layout.mix_cell_from_path(&[8]), Some((0, 0)));
        assert_eq!(layout.mix_cell_from_path(&[20]), Some((0, 12)));
        assert_eq!(layout.mix_cell_from_path(&[21]), Some((1, 0)));
        assert_eq!(layout.mix_cell_from_path(&[33]), Some((1, 12)));
        assert_eq!(layout.mix_cell_from_path(&[34]), None); // past matrix
        assert_eq!(layout.mix_cell_from_path(&[7]), None); // before first_mix
    }

    #[test]
    fn fails_when_physical_interface_missing() {
        let root = node_with_children("DEVICE", vec![node("CHANNEL"), node("MIX")]);
        let err = Layout::from_full_sync(&root).unwrap_err();
        assert_eq!(err, BuildError::MissingNode("PHYSICALINTERFACE under root"));
    }

    #[test]
    fn fails_when_fader_missing_inside_physical_interface() {
        let phys = node_with_children("PHYSICALINTERFACE", vec![node("HEADER")]);
        let mut children = vec![phys, node("CHANNEL")];
        for _ in 0..13 {
            children.push(node("MIX"));
        }
        let root = node_with_children("DEVICE", children);
        let err = Layout::from_full_sync(&root).unwrap_err();
        assert_eq!(
            err,
            BuildError::MissingNode("FADER under PHYSICALINTERFACE")
        );
    }

    #[test]
    fn fails_when_channel_missing() {
        let phys = node_with_children("PHYSICALINTERFACE", vec![node("FADER")]);
        let mut children = vec![phys];
        for _ in 0..13 {
            children.push(node("MIX"));
        }
        let root = node_with_children("DEVICE", children);
        let err = Layout::from_full_sync(&root).unwrap_err();
        assert_eq!(err, BuildError::MissingNode("CHANNEL under root"));
    }

    #[test]
    fn fails_when_mix_missing() {
        let phys = node_with_children("PHYSICALINTERFACE", vec![node("FADER")]);
        let root = node_with_children("DEVICE", vec![phys, node("CHANNEL")]);
        let err = Layout::from_full_sync(&root).unwrap_err();
        assert_eq!(err, BuildError::MissingNode("MIX under root"));
    }

    #[test]
    fn fails_when_mix_count_not_divisible_by_matrix_width() {
        let phys = node_with_children("PHYSICALINTERFACE", vec![node("FADER")]);
        let mut children = vec![phys, node("CHANNEL")];
        // 14 MIX nodes: not a clean multiple of 13.
        for _ in 0..14 {
            children.push(node("MIX"));
        }
        let root = node_with_children("DEVICE", children);
        let err = Layout::from_full_sync(&root).unwrap_err();
        match err {
            BuildError::ShapeMismatch { what, .. } => assert_eq!(what, "MIX"),
            other => panic!("expected ShapeMismatch, got {other:?}"),
        }
    }

    /// Build a SYSTEM node carrying the given board type, prepended to an
    /// otherwise-valid synthetic tree so `from_full_sync` succeeds.
    fn tree_with_system(props: Vec<Property>) -> Node {
        let phys = node_with_children("PHYSICALINTERFACE", vec![node("FADER")]);
        let system = Node {
            name: "SYSTEM".to_string(),
            properties: props,
            children: vec![],
        };
        let mut children = vec![system, phys, node("CHANNEL")];
        for _ in 0..13 {
            children.push(node("MIX"));
        }
        node_with_children("DEVICE", children)
    }

    #[test]
    fn model_defaults_to_pro2_without_system_node() {
        // The synthetic tree has no SYSTEM node.
        let layout = Layout::from_full_sync(&synthetic_tree()).unwrap();
        assert_eq!(layout.model(), DeviceModel::Pro2);
    }

    #[test]
    fn model_reads_board_type_from_system() {
        let pro2 = tree_with_system(vec![Property {
            name: "boardType".to_string(),
            value: Value::Int(0),
        }]);
        assert_eq!(
            Layout::from_full_sync(&pro2).unwrap().model(),
            DeviceModel::Pro2
        );

        let duo = tree_with_system(vec![Property {
            name: "boardType".to_string(),
            value: Value::Int(1),
        }]);
        assert_eq!(
            Layout::from_full_sync(&duo).unwrap().model(),
            DeviceModel::Duo
        );
    }

    #[test]
    fn model_falls_back_to_system_name() {
        // No boardType -> systemName decides.
        let duo = tree_with_system(vec![Property {
            name: "systemName".to_string(),
            value: Value::String("RODECaster Duo".to_string()),
        }]);
        assert_eq!(
            Layout::from_full_sync(&duo).unwrap().model(),
            DeviceModel::Duo
        );
    }
}
