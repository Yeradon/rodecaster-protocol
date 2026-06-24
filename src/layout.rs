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
//!
//! ## Addressing shapes
//!
//! Every node family reduces to one of three reusable shapes, so the per-family
//! path math is written once each and the [`Layout`] methods just delegate:
//!
//! - [`Singleton`]: one node, no index (`MASTERCHANNEL`, `OUTPUT`, `DUCKER`, ...).
//! - [`Indexed`]: a single-level run at root (`CHANNEL`, `INPUTSOURCE`, ...).
//! - [`TwoLevel`]: a run of children under a container (`FADER`, `PAD`).
//!
//! `MIX` is the one exception: it is a 2D source-major matrix with stride
//! arithmetic, so it keeps bespoke methods rather than a shared shape.

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

/// One addressable singleton node (no index): its tree position, or absent.
///
/// `MASTERCHANNEL`, `OUTPUT`, `DUCKER`, `RECORDER`, `PLAYER` and `GUI` are all
/// one-of-a-kind nodes addressed purely by position. `None` means the fullSync
/// did not carry the node (synthetic or partial trees).
#[derive(Debug, Clone, PartialEq)]
struct Singleton {
    idx: Option<u32>,
}

impl Singleton {
    /// Root-down single-element path to the node, if present.
    fn path(&self) -> Option<Vec<u32>> {
        self.idx.map(|p| vec![p])
    }

    /// True if this single-level path points at the node.
    fn is_path(&self, path: &[u32]) -> bool {
        matches!((self.idx, path), (Some(i), [p]) if *p == i)
    }
}

/// A contiguous run of same-named nodes under root, addressed by a single-level
/// path `[first + n]`. `first` is `None` when the run is absent (which, since a
/// found node always has at least one consecutive entry, is the same as
/// `count == 0`).
///
/// `CHANNEL`, `INPUTSOURCE`, `HEADPHONE` and the root `EFFECTS_PARAMETERS` run
/// share this shape: the ordinal within the run is the logical index.
#[derive(Debug, Clone, PartialEq)]
struct Indexed {
    first: Option<u32>,
    count: u8,
}

impl Indexed {
    /// Root-down path to the `n`th node in the run, or `None` if `n` is past the
    /// run or the run is absent.
    fn path(&self, n: u8) -> Option<Vec<u32>> {
        let first = self.first?;
        if n >= self.count {
            return None;
        }
        Some(vec![first + n as u32])
    }

    /// Inverse of [`Indexed::path`]: the ordinal this single-level path
    /// identifies, if it falls inside the run.
    fn index_from_path(&self, path: &[u32]) -> Option<u8> {
        let first = self.first?;
        if path.len() != 1 {
            return None;
        }
        let p = path[0];
        if p < first {
            return None;
        }
        let n = p - first;
        if n >= self.count as u32 {
            return None;
        }
        Some(n as u8)
    }
}

/// A contiguous run of same-named child nodes under a parent container,
/// addressed by a two-level path `[parent, first + n]`. `parent` is `None` when
/// the container is absent; `count` can be 0 even when the container exists
/// (e.g. a `SOUNDPADS` node with no `PAD` children), so parent presence is
/// tracked independently of the run length.
///
/// `FADER` (under `PHYSICALINTERFACE`) and `PAD` (under `SOUNDPADS`) share this
/// shape.
#[derive(Debug, Clone, PartialEq)]
struct TwoLevel {
    parent: Option<u32>,
    first: u32,
    count: u8,
}

impl TwoLevel {
    /// Root-down two-level path to the `n`th child, or `None` if `n` is past the
    /// run or the container is absent.
    fn path(&self, n: u8) -> Option<Vec<u32>> {
        let parent = self.parent?;
        if n >= self.count {
            return None;
        }
        Some(vec![parent, self.first + n as u32])
    }

    /// Inverse of [`TwoLevel::path`]: the ordinal this two-level path identifies,
    /// if its parent matches and it falls inside the run.
    fn index_from_path(&self, path: &[u32]) -> Option<u8> {
        let parent = self.parent?;
        if path.len() != 2 || path[0] != parent {
            return None;
        }
        let p = path[1];
        if p < self.first {
            return None;
        }
        let n = p - self.first;
        if n >= self.count as u32 {
            return None;
        }
        Some(n as u8)
    }
}

/// Tree positions of the addressable node families. All fields are
/// discovered, not hardcoded.
#[derive(Debug, Clone, PartialEq)]
pub struct Layout {
    model: DeviceModel,
    // CHANNEL: a required single-level run at root (so `first` is always `Some`).
    channel: Indexed,
    // MIX: a 2D source-major matrix at root. Bespoke stride arithmetic, so it is
    // not one of the generic addressing shapes. `source_count` is the first
    // matrix dimension; the second is MIX_COUNT_PER_SOURCE.
    first_mix: u32,
    source_count: u8,
    // FADER under PHYSICALINTERFACE: a required two-level run.
    fader: TwoLevel,
    // INPUTSOURCE / HEADPHONE / root EFFECTS_PARAMETERS: optional single-level
    // runs at root. Absent (synthetic or partial trees) -> `first` is `None` and
    // addressing simply returns `None` (no panic, no misroute).
    input_source: Indexed,
    headphone: Indexed,
    effects: Indexed,
    // MASTERCHANNEL / OUTPUT / DUCKER / RECORDER / PLAYER / GUI: root singletons,
    // each addressed by position with no index. Optional for the same reason.
    master_channel: Singleton,
    output: Singleton,
    ducker: Singleton,
    recorder: Singleton,
    player: Singleton,
    gui: Singleton,
    // PAD under SOUNDPADS: an optional two-level run (mirrors FADER). The
    // container can exist with zero pads, so its presence is tracked separately
    // from the run length.
    pad: TwoLevel,
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

        // PHYSICALINTERFACE under root, with FADER children inside it (required,
        // two-level).
        let physical_interface_idx = position_of_named_child(root, "PHYSICALINTERFACE")
            .ok_or(BuildError::MissingNode("PHYSICALINTERFACE under root"))?;
        let phys = &root.children[physical_interface_idx as usize];
        let first_fader_in_phys = position_of_named_child(phys, "FADER")
            .ok_or(BuildError::MissingNode("FADER under PHYSICALINTERFACE"))?;
        let fader = TwoLevel {
            parent: Some(physical_interface_idx),
            first: first_fader_in_phys,
            count: count_consecutive_named(phys, first_fader_in_phys, "FADER"),
        };

        // CHANNEL nodes under root (required, single-level).
        let first_channel = position_of_named_child(root, "CHANNEL")
            .ok_or(BuildError::MissingNode("CHANNEL under root"))?;
        let channel = Indexed {
            first: Some(first_channel),
            count: count_consecutive_named(root, first_channel, "CHANNEL"),
        };

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

        // INPUTSOURCE nodes under root (optional, single-level). The first
        // contiguous run holds the addressable sources; its ordinal == the
        // device's `inputId` == `Source::to_protocol`. A second run (rcSync /
        // streamer-x placeholders) follows on some devices and is deliberately
        // not counted (discover_run takes only the first run).
        let input_source = discover_run(root, "INPUTSOURCE");

        // HEADPHONE nodes under root (optional, single-level, multi-instance).
        // One node per physical headphone jack; the ordinal in the run is the
        // headphone index.
        let headphone = discover_run(root, "HEADPHONE");

        // EFFECTS_PARAMETERS nodes under root (optional, single-level). The first
        // contiguous run at root is the per-channel-strip effects slots; the
        // ordinal in the run is the slot index (== the node's `effectsIdx`). The
        // PADEFFECTS node nests its own EFFECTS_PARAMETERS set elsewhere in the
        // tree; only the root run is counted here.
        let effects = discover_run(root, "EFFECTS_PARAMETERS");

        // MASTERCHANNEL, OUTPUT, DUCKER, RECORDER, PLAYER and GUI are singletons
        // under root; record each position if present (None on synthetic/partial
        // trees).
        let master_channel = Singleton {
            idx: position_of_named_child(root, "MASTERCHANNEL"),
        };
        let output = Singleton {
            idx: position_of_named_child(root, "OUTPUT"),
        };
        let ducker = Singleton {
            idx: position_of_named_child(root, "DUCKER"),
        };
        let recorder = Singleton {
            idx: position_of_named_child(root, "RECORDER"),
        };
        let player = Singleton {
            idx: position_of_named_child(root, "PLAYER"),
        };
        let gui = Singleton {
            idx: position_of_named_child(root, "GUI"),
        };

        // SOUNDPADS container under root (optional). Inside it, PAD nodes form a
        // contiguous run; the ordinal in that run is the pad index (== the pad's
        // own `padIdx`). Two-level addressing, so we record the container index
        // plus the first-PAD offset and run length within it.
        let pad = match position_of_named_child(root, "SOUNDPADS") {
            Some(sp_idx) => {
                let sp = &root.children[sp_idx as usize];
                match position_of_named_child(sp, "PAD") {
                    Some(first) => TwoLevel {
                        parent: Some(sp_idx),
                        first,
                        count: count_consecutive_named(sp, first, "PAD"),
                    },
                    None => TwoLevel {
                        parent: Some(sp_idx),
                        first: 0,
                        count: 0,
                    },
                }
            }
            None => TwoLevel {
                parent: None,
                first: 0,
                count: 0,
            },
        };

        Ok(Layout {
            model,
            channel,
            first_mix,
            source_count,
            fader,
            input_source,
            headphone,
            effects,
            master_channel,
            output,
            ducker,
            recorder,
            player,
            gui,
            pad,
        })
    }

    /// Which RODECaster this layout was discovered from. Selects the per-model
    /// [`crate::Fader`] mapping when resolving typed commands and events.
    pub fn model(&self) -> DeviceModel {
        self.model
    }

    pub fn physical_interface_idx(&self) -> u32 {
        // FADER (and thus its PHYSICALINTERFACE parent) is required, so any built
        // Layout has a parent here.
        self.fader
            .parent
            .expect("PHYSICALINTERFACE present in every built Layout")
    }
    pub fn first_fader_in_phys(&self) -> u32 {
        self.fader.first
    }
    pub fn fader_count(&self) -> u8 {
        self.fader.count
    }
    pub fn first_channel(&self) -> u32 {
        // CHANNEL is required, so any built Layout has a first index here.
        self.channel
            .first
            .expect("CHANNEL present in every built Layout")
    }
    pub fn channel_count(&self) -> u8 {
        self.channel.count
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
    /// Index of the first `INPUTSOURCE` node under root, if the fullSync had any.
    pub fn first_input_source(&self) -> Option<u32> {
        self.input_source.first
    }
    /// Number of addressable `INPUTSOURCE` nodes (the first contiguous run).
    pub fn input_source_count(&self) -> u8 {
        self.input_source.count
    }
    /// Index of the single `MASTERCHANNEL` node under root, if present.
    pub fn master_channel(&self) -> Option<u32> {
        self.master_channel.idx
    }
    /// Index of the single `OUTPUT` node under root, if present.
    pub fn output(&self) -> Option<u32> {
        self.output.idx
    }
    /// Index of the single `DUCKER` node under root, if present.
    pub fn ducker(&self) -> Option<u32> {
        self.ducker.idx
    }
    /// Index of the single `RECORDER` node under root, if present.
    pub fn recorder(&self) -> Option<u32> {
        self.recorder.idx
    }
    /// Index of the single `PLAYER` node under root, if present.
    pub fn player(&self) -> Option<u32> {
        self.player.idx
    }
    /// Index of the first `HEADPHONE` node under root, if the fullSync had any.
    pub fn first_headphone(&self) -> Option<u32> {
        self.headphone.first
    }
    /// Number of addressable `HEADPHONE` nodes (one per physical jack).
    pub fn headphone_count(&self) -> u8 {
        self.headphone.count
    }
    /// Index of the first root `EFFECTS_PARAMETERS` node, if the fullSync had any.
    pub fn first_effects(&self) -> Option<u32> {
        self.effects.first
    }
    /// Number of addressable `EFFECTS_PARAMETERS` slots (the first root run).
    pub fn effects_count(&self) -> u8 {
        self.effects.count
    }
    /// Index of the single `GUI` (front-panel UI state) node under root, if present.
    pub fn gui(&self) -> Option<u32> {
        self.gui.idx
    }
    /// Index of the `SOUNDPADS` container node under root, if present.
    pub fn soundpads(&self) -> Option<u32> {
        self.pad.parent
    }
    /// Offset of the first `PAD` node inside the `SOUNDPADS` container.
    pub fn first_pad(&self) -> u32 {
        self.pad.first
    }
    /// Number of `PAD` nodes in the contiguous run inside `SOUNDPADS`.
    pub fn pad_count(&self) -> u8 {
        self.pad.count
    }

    /// Root-down path to the `n`th `CHANNEL` node (single-level).
    /// Used for `channelOutputMute`, `channelCueEnable`, `channelInputSource`.
    pub fn channel_path(&self, n: u8) -> Option<Vec<u32>> {
        self.channel.path(n)
    }

    /// Root-down path to the `n`th `FADER` strip inside `PHYSICALINTERFACE`
    /// (two-level). Used for `faderLevel`.
    pub fn fader_path(&self, n: u8) -> Option<Vec<u32>> {
        self.fader.path(n)
    }

    /// Root-down path to the `n`th `PAD` strip inside `SOUNDPADS` (two-level).
    /// Used for the `pad*` properties. `None` if `n` is past the discovered run
    /// or no SOUNDPADS container was present.
    pub fn pad_path(&self, n: u8) -> Option<Vec<u32>> {
        self.pad.path(n)
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

    /// Root-down path to the `n`th `INPUTSOURCE` node (single-level). `n` is the
    /// source ordinal (== device `inputId` == [`crate::Source::to_protocol`]).
    /// Used for the input* preamp properties (gain, power, mic type, phase, ...).
    /// `None` if `n` is past the discovered run or no INPUTSOURCE was present.
    pub fn input_source_path(&self, n: u8) -> Option<Vec<u32>> {
        self.input_source.path(n)
    }

    /// Inverse of `input_source_path`: which input-source ordinal does this
    /// single-level path identify, if any?
    pub fn input_source_index_from_path(&self, path: &[u32]) -> Option<u8> {
        self.input_source.index_from_path(path)
    }

    /// Root-down path to the single `MASTERCHANNEL` node, if present. No index:
    /// there is exactly one master bus. Used for the master* properties
    /// (Compellor, delay).
    pub fn master_channel_path(&self) -> Option<Vec<u32>> {
        self.master_channel.path()
    }

    /// True if this single-level path points at the `MASTERCHANNEL` node.
    pub fn is_master_channel_path(&self, path: &[u32]) -> bool {
        self.master_channel.is_path(path)
    }

    /// Root-down path to the single `OUTPUT` node, if present. No index: there
    /// is exactly one output bus. Used for the output*/recording* properties.
    pub fn output_path(&self) -> Option<Vec<u32>> {
        self.output.path()
    }

    /// True if this single-level path points at the `OUTPUT` node.
    pub fn is_output_path(&self, path: &[u32]) -> bool {
        self.output.is_path(path)
    }

    /// Root-down path to the single `DUCKER` node, if present. No index: there is
    /// exactly one ducker. Used for `duckerDepth`.
    pub fn ducker_path(&self) -> Option<Vec<u32>> {
        self.ducker.path()
    }

    /// True if this single-level path points at the `DUCKER` node.
    pub fn is_ducker_path(&self, path: &[u32]) -> bool {
        self.ducker.is_path(path)
    }

    /// Root-down path to the single `RECORDER` node, if present. No index: there
    /// is exactly one recorder. Used for the record*/request* transport props.
    pub fn recorder_path(&self) -> Option<Vec<u32>> {
        self.recorder.path()
    }

    /// True if this single-level path points at the `RECORDER` node.
    pub fn is_recorder_path(&self, path: &[u32]) -> bool {
        self.recorder.is_path(path)
    }

    /// Root-down path to the single `PLAYER` node, if present. No index: there is
    /// exactly one long-form player. Used for the player* transport/file props.
    pub fn player_path(&self) -> Option<Vec<u32>> {
        self.player.path()
    }

    /// True if this single-level path points at the `PLAYER` node.
    pub fn is_player_path(&self, path: &[u32]) -> bool {
        self.player.is_path(path)
    }

    /// Root-down path to the single `GUI` node, if present. No index: there is
    /// exactly one front-panel UI-state node. Used for the gui* / screen* /
    /// touchscreen-EQ-focus properties.
    pub fn gui_path(&self) -> Option<Vec<u32>> {
        self.gui.path()
    }

    /// True if this single-level path points at the `GUI` node.
    pub fn is_gui_path(&self, path: &[u32]) -> bool {
        self.gui.is_path(path)
    }

    /// Root-down path to the `n`th `HEADPHONE` node (single-level). `n` is the
    /// headphone-jack index. `None` if `n` is past the discovered run or no
    /// HEADPHONE node was present.
    pub fn headphone_path(&self, n: u8) -> Option<Vec<u32>> {
        self.headphone.path(n)
    }

    /// Inverse of `headphone_path`: which headphone index does this single-level
    /// path identify, if any?
    pub fn headphone_index_from_path(&self, path: &[u32]) -> Option<u8> {
        self.headphone.index_from_path(path)
    }

    /// Root-down path to the `n`th root `EFFECTS_PARAMETERS` node (single-level).
    /// `n` is the effects-slot index (== the node's `effectsIdx`). `None` if `n`
    /// is past the discovered run or no root EFFECTS_PARAMETERS node was present.
    pub fn effects_path(&self, n: u8) -> Option<Vec<u32>> {
        self.effects.path(n)
    }

    /// Inverse of `effects_path`: which effects-slot index does this single-level
    /// path identify, if any?
    pub fn effects_index_from_path(&self, path: &[u32]) -> Option<u8> {
        self.effects.index_from_path(path)
    }

    /// Inverse of `channel_path`: which channel index does this single-level
    /// path identify, if any?
    pub fn channel_index_from_path(&self, path: &[u32]) -> Option<u8> {
        self.channel.index_from_path(path)
    }

    /// Inverse of `fader_path`: which fader index does this two-level path
    /// identify, if any?
    pub fn fader_index_from_path(&self, path: &[u32]) -> Option<u8> {
        self.fader.index_from_path(path)
    }

    /// Inverse of `pad_path`: which pad index does this two-level path identify,
    /// if any?
    pub fn pad_index_from_path(&self, path: &[u32]) -> Option<u8> {
        self.pad.index_from_path(path)
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

/// Discover the first contiguous run of `name` children under `parent` as an
/// [`Indexed`] addressing shape. Absent -> `first: None, count: 0`.
fn discover_run(parent: &Node, name: &str) -> Indexed {
    match position_of_named_child(parent, name) {
        Some(first) => Indexed {
            first: Some(first),
            count: count_consecutive_named(parent, first, name),
        },
        None => Indexed {
            first: None,
            count: 0,
        },
    }
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
#[path = "layout_tests.rs"]
mod tests;
