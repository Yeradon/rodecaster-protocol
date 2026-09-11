//! Dynamic device topology discovery and address translation.
//!
//! RØDECaster devices do not use fixed, static wire addresses across models
//! and firmware versions. Instead, the console sends a full state tree
//! (`fullSync`) when connected.
//!
//! [`Layout`] inspects this tree to discover where physical faders, channels,
//! audio inputs, and mix routing nodes reside. High-level commands and events
//! use [`Layout`] to translate between human-readable domain names (such as
//! [`crate::Fader::Physical1`]) and the exact numeric node indices on the wire.
//!
//! Most applications do not need to construct or query [`Layout`] directly:
//! [`crate::ProtocolSession`] manages the active layout automatically.

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
/// `MASTERCHANNEL`, `OUTPUT`, `DUCKER`, `RECORDER`, `PLAYER`, `GUI` and `SYSTEM`
/// are all one-of-a-kind nodes addressed purely by position. `None` means the
/// fullSync did not carry the node (synthetic or partial trees).
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
    // MASTERCHANNEL / OUTPUT / DUCKER / RECORDER / PLAYER / GUI / SYSTEM: root
    // singletons, each addressed by position with no index. Optional for the
    // same reason.
    master_channel: Singleton,
    output: Singleton,
    ducker: Singleton,
    recorder: Singleton,
    player: Singleton,
    gui: Singleton,
    system: Singleton,
    // PAD under SOUNDPADS: an optional two-level run (mirrors FADER). The
    // container can exist with zero pads, so its presence is tracked separately
    // from the run length.
    pad: TwoLevel,
    // SIP subsystem. SIPCALLING and SIPADVANCED are singletons at root; the
    // SIPCALLSLOTS run holds one node per configured call slot (three on the
    // Duo); SIPREGISTRATION nodes are children of SIPCALLING (two on the Duo).
    sip_calling: Singleton,
    sip_advanced: Singleton,
    sip_call_slots: Indexed,
    sip_registration: TwoLevel,
    // TEST: factory-diagnostic singleton at root.
    test: Singleton,
    // PADRECORDER: a run at root (one node per pad recorder instance).
    pad_recorder: Indexed,
    // FXPRESET: two-level under the FXPRESETS container at root.
    fx_preset: TwoLevel,
    // Singletons at root.
    network: Singleton,
    audio: Singleton,
    build: Singleton,
    app: Singleton,
    theme: Singleton,
    current_show: Singleton,
    show_control: Singleton,
    recordings: Singleton,
    radio: Singleton,
    // Container-level and indexed runs.
    show: TwoLevel,
    recording: TwoLevel,
    storage_volume: Indexed,
    radio_tx: Indexed,
    radio_rx: Indexed,
    wifi_scan_result: Indexed,
    streamerx_mix_preset: Indexed,
    streamerx_stream_mix: Indexed,
    rcsync_mix: Indexed,
    mix_minuses: Indexed,
}

macro_rules! impl_singleton {
    ($field:ident, $path_fn:ident, $is_path_fn:ident) => {
        pub fn $path_fn(&self) -> Option<Vec<u32>> {
            self.$field.path()
        }
        pub fn $is_path_fn(&self, path: &[u32]) -> bool {
            self.$field.is_path(path)
        }
    };
}

macro_rules! impl_run {
    ($field:ident, $path_fn:ident, $index_fn:ident, $count_fn:ident) => {
        pub fn $path_fn(&self, n: u8) -> Option<Vec<u32>> {
            self.$field.path(n)
        }
        pub fn $index_fn(&self, path: &[u32]) -> Option<u8> {
            self.$field.index_from_path(path)
        }
        pub fn $count_fn(&self) -> u8 {
            self.$field.count
        }
    };
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

        // MASTERCHANNEL, OUTPUT, DUCKER, RECORDER, PLAYER, GUI and SYSTEM are
        // singletons under root; record each position if present (None on
        // synthetic/partial trees).
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
        // SYSTEM is the device-wide state singleton. `detect_model` reads the
        // same node by name independently; this records its position so SYSTEM
        // properties are addressable like any other singleton family.
        let system = Singleton {
            idx: position_of_named_child(root, "SYSTEM"),
        };

        // TEST diagnostic singleton at root.
        let test = Singleton {
            idx: position_of_named_child(root, "TEST"),
        };

        // PADRECORDER: a run at root (one per pad recorder instance).
        let pad_recorder = discover_run(root, "PADRECORDER");

        // FXPRESET: two-level under FXPRESETS container at root.
        let fx_preset = discover_two_level(root, "FXPRESETS", "FXPRESET");

        // SIP subsystem discovery.
        let sip_calling = Singleton {
            idx: position_of_named_child(root, "SIPCALLING"),
        };
        let sip_advanced = Singleton {
            idx: position_of_named_child(root, "SIPADVANCED"),
        };
        let sip_call_slots = discover_run(root, "SIPCALLSLOTS");
        let sip_registration = discover_two_level(root, "SIPCALLING", "SIPREGISTRATION");

        // SOUNDPADS container under root.
        let pad = discover_two_level(root, "SOUNDPADS", "PAD");

        // Root singletons.
        let network = Singleton {
            idx: position_of_named_child(root, "NETWORK"),
        };
        let audio = Singleton {
            idx: position_of_named_child(root, "AUDIO"),
        };
        let build = Singleton {
            idx: position_of_named_child(root, "BUILD"),
        };
        let app = Singleton {
            idx: position_of_named_child(root, "APP"),
        };
        let theme = Singleton {
            idx: position_of_named_child(root, "THEME"),
        };
        let current_show = Singleton {
            idx: position_of_named_child(root, "CURRENTSHOW"),
        };
        let show_control = Singleton {
            idx: position_of_named_child(root, "SHOWCONTROL"),
        };
        let recordings = Singleton {
            idx: position_of_named_child(root, "RECORDINGS"),
        };
        let radio = Singleton {
            idx: position_of_named_child(root, "RADIO"),
        };

        // Container-nested runs.
        let show = discover_two_level(root, "SHOWS", "SHOW");
        let recording = discover_two_level(root, "RECORDINGS", "RECORDING");

        // Single-level runs at root.
        let storage_volume = discover_run(root, "STORAGEVOLUME");
        let radio_tx = discover_run(root, "RADIOTX");
        let radio_rx = discover_run(root, "RADIORX");
        let wifi_scan_result = discover_run(root, "WIFISCANRESULT");
        let streamerx_mix_preset = discover_run(root, "STREAMERXMIXPRESET");
        let streamerx_stream_mix = discover_run(root, "STREAMERXSTREAMMIX");
        let rcsync_mix = discover_run(root, "RCSYNCMIX");
        let mix_minuses = discover_run(root, "MIXMINUSES");

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
            system,
            pad,
            sip_calling,
            sip_advanced,
            sip_call_slots,
            sip_registration,
            test,
            pad_recorder,
            fx_preset,
            network,
            audio,
            build,
            app,
            theme,
            current_show,
            show_control,
            recordings,
            radio,
            show,
            recording,
            storage_volume,
            radio_tx,
            radio_rx,
            wifi_scan_result,
            streamerx_mix_preset,
            streamerx_stream_mix,
            rcsync_mix,
            mix_minuses,
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
    /// Index of the single `SYSTEM` (device-wide state) node under root, if present.
    pub fn system(&self) -> Option<u32> {
        self.system.idx
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

    // Core hardware and routing paths

    /// Root-down path to the `n`th `CHANNEL` node (single-level).
    pub fn channel_path(&self, n: u8) -> Option<Vec<u32>> {
        self.channel.path(n)
    }

    /// Inverse of `channel_path`: channel index from single-level path.
    pub fn channel_index_from_path(&self, path: &[u32]) -> Option<u8> {
        self.channel.index_from_path(path)
    }

    /// Root-down path to the `n`th `FADER` strip inside `PHYSICALINTERFACE` (two-level).
    pub fn fader_path(&self, n: u8) -> Option<Vec<u32>> {
        self.fader.path(n)
    }

    /// Inverse of `fader_path`: fader index from two-level path.
    pub fn fader_index_from_path(&self, path: &[u32]) -> Option<u8> {
        self.fader.index_from_path(path)
    }

    /// Root-down path to the `n`th `PAD` strip inside `SOUNDPADS` (two-level).
    pub fn pad_path(&self, n: u8) -> Option<Vec<u32>> {
        self.pad.path(n)
    }

    /// Inverse of `pad_path`: pad index from two-level path.
    pub fn pad_index_from_path(&self, path: &[u32]) -> Option<u8> {
        self.pad.index_from_path(path)
    }

    /// Root-down path to the `n`th `INPUTSOURCE` node (single-level).
    pub fn input_source_path(&self, n: u8) -> Option<Vec<u32>> {
        self.input_source.path(n)
    }

    /// Inverse of `input_source_path`: which input-source ordinal does this path identify?
    pub fn input_source_index_from_path(&self, path: &[u32]) -> Option<u8> {
        self.input_source.index_from_path(path)
    }

    /// Root-down path to the `n`th `HEADPHONE` node (single-level).
    pub fn headphone_path(&self, n: u8) -> Option<Vec<u32>> {
        self.headphone.path(n)
    }

    /// Inverse of `headphone_path`: headphone index from single-level path.
    pub fn headphone_index_from_path(&self, path: &[u32]) -> Option<u8> {
        self.headphone.index_from_path(path)
    }

    /// Root-down path to the `n`th root `EFFECTS_PARAMETERS` node (single-level).
    pub fn effects_path(&self, n: u8) -> Option<Vec<u32>> {
        self.effects.path(n)
    }

    /// Inverse of `effects_path`: effects-slot index from single-level path.
    pub fn effects_index_from_path(&self, path: &[u32]) -> Option<u8> {
        self.effects.index_from_path(path)
    }

    /// Root-down path to the `MIX` cell at (`source`, `mix`), source-major layout.
    pub fn mix_cell_path(&self, source: u8, mix: u8) -> Option<Vec<u32>> {
        if source >= self.source_count || mix >= MIX_COUNT_PER_SOURCE {
            return None;
        }
        Some(vec![
            self.first_mix + source as u32 * MIX_COUNT_PER_SOURCE as u32 + mix as u32,
        ])
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

    // Singleton node accessors
    impl_singleton!(master_channel, master_channel_path, is_master_channel_path);
    impl_singleton!(output, output_path, is_output_path);
    impl_singleton!(ducker, ducker_path, is_ducker_path);
    impl_singleton!(recorder, recorder_path, is_recorder_path);
    impl_singleton!(player, player_path, is_player_path);
    impl_singleton!(gui, gui_path, is_gui_path);
    impl_singleton!(system, system_path, is_system_path);
    impl_singleton!(sip_calling, sip_calling_path, is_sip_calling_path);
    impl_singleton!(sip_advanced, sip_advanced_path, is_sip_advanced_path);
    impl_singleton!(test, test_path, is_test_path);
    impl_singleton!(network, network_path, is_network_path);
    impl_singleton!(audio, audio_path, is_audio_path);
    impl_singleton!(build, build_path, is_build_path);
    impl_singleton!(app, app_path, is_app_path);
    impl_singleton!(theme, theme_path, is_theme_path);
    impl_singleton!(current_show, current_show_path, is_current_show_path);
    impl_singleton!(show_control, show_control_path, is_show_control_path);
    impl_singleton!(recordings, recordings_path, is_recordings_path);
    impl_singleton!(radio, radio_path, is_radio_path);

    // Indexed and container-nested run accessors
    impl_run!(
        sip_call_slots,
        sip_call_slots_path,
        sip_call_slots_index_from_path,
        sip_call_slots_count
    );
    impl_run!(
        sip_registration,
        sip_registration_path,
        sip_registration_index_from_path,
        sip_registration_count
    );
    impl_run!(
        pad_recorder,
        pad_recorder_path,
        pad_recorder_index_from_path,
        pad_recorder_count
    );
    impl_run!(
        fx_preset,
        fx_preset_path,
        fx_preset_index_from_path,
        fx_preset_count
    );
    impl_run!(show, show_path, show_index_from_path, show_count);
    impl_run!(
        recording,
        recording_path,
        recording_index_from_path,
        recording_count
    );
    impl_run!(
        storage_volume,
        storage_volume_path,
        storage_volume_index_from_path,
        storage_volume_count
    );
    impl_run!(
        radio_tx,
        radio_tx_path,
        radio_tx_index_from_path,
        radio_tx_count
    );
    impl_run!(
        radio_rx,
        radio_rx_path,
        radio_rx_index_from_path,
        radio_rx_count
    );
    impl_run!(
        wifi_scan_result,
        wifi_scan_result_path,
        wifi_scan_result_index_from_path,
        wifi_scan_result_count
    );
    impl_run!(
        streamerx_mix_preset,
        streamerx_mix_preset_path,
        streamerx_mix_preset_index_from_path,
        streamerx_mix_preset_count
    );
    impl_run!(
        streamerx_stream_mix,
        streamerx_stream_mix_path,
        streamerx_stream_mix_index_from_path,
        streamerx_stream_mix_count
    );
    impl_run!(
        rcsync_mix,
        rcsync_mix_path,
        rcsync_mix_index_from_path,
        rcsync_mix_count
    );
    impl_run!(
        mix_minuses,
        mix_minuses_path,
        mix_minuses_index_from_path,
        mix_minuses_count
    );
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

/// Discover a two-level run of `child_name` nodes inside a `container_name` parent
/// as a [`TwoLevel`] addressing shape. Absent -> `parent: None, first: 0, count: 0`.
fn discover_two_level(root: &Node, container_name: &str, child_name: &str) -> TwoLevel {
    match position_of_named_child(root, container_name) {
        Some(parent) => {
            let container = &root.children[parent as usize];
            match position_of_named_child(container, child_name) {
                Some(first) => TwoLevel {
                    parent: Some(parent),
                    first,
                    count: count_consecutive_named(container, first, child_name),
                },
                None => TwoLevel {
                    parent: Some(parent),
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
