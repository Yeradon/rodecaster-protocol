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
use crate::names::{
    ChannelParam, DeviceModel, DuckerParam, EffectsParam, Fader, GuiParam, HeadphoneParam,
    InputSourceParam, MasterParam, MixOutput, OutputParam, PadParam, PlayerParam, RecorderParam,
    Source, SystemParam,
};
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
        fader: Fader,
        muted: bool,
    },
    FaderCueChanged {
        fader: Fader,
        enabled: bool,
    },
    /// Virtual fader level (0..127 MIDI scale).
    FaderLevelChanged {
        fader: Fader,
        level: u8,
    },
    /// A fader strip was touched (the device's `encoderSignal`). The wire
    /// addresses it by raw fader index (single-level path, no base offset).
    FaderTouched {
        fader: Fader,
    },
    /// A fader's input-source assignment changed (`channelInputSource` echo,
    /// resolved at the stride-6 echo addressing — see module docs). `source`
    /// is `None` when the slot was unassigned (wire value < 0).
    FaderAssignmentChanged {
        fader: Fader,
        source: Option<Source>,
    },

    /// A channel-strip parameter changed: the EQ, compressor, de-esser, noise
    /// gate, HPF, aphex, pan, tone or preamp controls for one fader. These all
    /// live as flat properties on the device's `CHANNEL` node, so they resolve
    /// to a [`Fader`] through the same path as mute/cue.
    ///
    /// `param` is the typed property identity ([`ChannelParam`], a ground-truth
    /// wire name); `value` is the self-describing wire value (its type comes
    /// from the JUCE marker, not from a guess). A property not yet given a typed
    /// identifier arrives as [`ChannelParam::Other`] rather than collapsing into
    /// [`DeviceEvent::Unknown`], so the whole strip is addressable today and new
    /// firmware properties still surface with their fader resolved.
    ChannelParamChanged {
        fader: Fader,
        param: ChannelParam,
        value: Value,
    },

    /// An input-source parameter changed: the preamp gain, 48V power, mic type,
    /// phase, colour, wireless serial or SIP/RCV routing for one source. These
    /// live on the device's `INPUTSOURCE` node (one per [`Source`]), addressed
    /// independently of any fader assignment, so this resolves to a [`Source`],
    /// not a [`Fader`].
    ///
    /// `param` is the typed property identity ([`InputSourceParam`]); `value` is
    /// the self-describing wire value. An un-typed property arrives as
    /// [`InputSourceParam::Other`] rather than collapsing into
    /// [`DeviceEvent::Unknown`].
    InputSourceParamChanged {
        source: Source,
        param: InputSourceParam,
        value: Value,
    },

    /// A master-bus parameter changed: the master Compellor (compressor) or the
    /// master delay, on the single `MASTERCHANNEL` node. There is exactly one
    /// master bus, so this carries no addressing key. `param` is the typed
    /// property identity ([`MasterParam`]); `value` is the self-describing wire
    /// value. An un-typed property arrives as [`MasterParam::Other`] rather than
    /// collapsing into [`DeviceEvent::Unknown`].
    MasterParamChanged {
        param: MasterParam,
        value: Value,
    },

    /// An output-bus parameter changed: a monitor/Bluetooth level or mute, the
    /// multi-out mode, or a recording-bus flag, on the single `OUTPUT` node.
    /// Key-less for the same reason as [`DeviceEvent::MasterParamChanged`].
    /// `param` is the typed property identity ([`OutputParam`]); an un-typed
    /// property arrives as [`OutputParam::Other`].
    OutputParamChanged {
        param: OutputParam,
        value: Value,
    },

    /// The auto-duck depth changed, on the single `DUCKER` node. Key-less:
    /// there is exactly one ducker. `param` is the typed property identity
    /// ([`DuckerParam`]); an un-typed property arrives as [`DuckerParam::Other`].
    DuckerParamChanged {
        param: DuckerParam,
        value: Value,
    },

    /// A recorder transport property changed, on the single `RECORDER` node:
    /// either read-back state (current state, elapsed ms, byte rate) or the
    /// request* command channel echoing back. Key-less: one recorder. `param`
    /// is the typed property identity ([`RecorderParam`]); an un-typed property
    /// arrives as [`RecorderParam::Other`].
    RecorderParamChanged {
        param: RecorderParam,
        value: Value,
    },

    /// A long-form player property changed, on the single `PLAYER` node:
    /// transport (state, speed, progress), the loaded file, or the in/out +
    /// fade envelope. Key-less: one player. `param` is the typed property
    /// identity ([`PlayerParam`]); an un-typed property arrives as
    /// [`PlayerParam::Other`].
    PlayerParamChanged {
        param: PlayerParam,
        value: Value,
    },

    /// A per-headphone property changed (`headphoneColour` / `headphoneType`),
    /// on one `HEADPHONE` node. The device has one node per physical headphone
    /// jack, so this carries the `headphone` index. `param` is the typed
    /// property identity ([`HeadphoneParam`]); an un-typed property arrives as
    /// [`HeadphoneParam::Other`].
    HeadphoneParamChanged {
        headphone: u8,
        param: HeadphoneParam,
        value: Value,
    },

    /// A per-slot effects parameter changed (reverb, echo/delay, pitch shift,
    /// distortion, robot or voice-disguise control), on one root
    /// `EFFECTS_PARAMETERS` node. The device exposes a contiguous run of these,
    /// one per channel-strip effects slot, so this carries the `effects` slot
    /// index. `param` is the typed property identity ([`EffectsParam`]); an
    /// un-typed property arrives as [`EffectsParam::Other`]. (The pad/sample
    /// effects nested under `PADEFFECTS` are a separate addressing context, not
    /// resolved here.)
    EffectsParamChanged {
        effects: u8,
        param: EffectsParam,
        value: Value,
    },

    /// A front-panel UI parameter changed (display / button brightness, selected
    /// pad bank, metering mode, touchscreen EQ-band focus, ...), on the single
    /// root `GUI` node. Key-less: there is exactly one GUI node. `param` is the
    /// typed property identity ([`GuiParam`]); an un-typed property arrives as
    /// [`GuiParam::Other`]. This is UI state, distinct from the
    /// [`crate::Command::ScreenTouched`] wake-the-display pulse.
    GuiParamChanged {
        param: GuiParam,
        value: Value,
    },

    /// A sound-pad parameter changed (colour / name / type / loaded sample /
    /// transport / gain / envelope / mixer routing / effect / SIP / MIDI-trigger
    /// control), on one `PAD` node inside the `SOUNDPADS` container. The device
    /// exposes a contiguous run of these (one per pad), so this carries the `pad`
    /// index. `param` is the typed property identity ([`PadParam`]); an un-typed
    /// property arrives as [`PadParam::Other`]. (The pad recorder, pad effects and
    /// FX presets live in separate sibling nodes, not resolved here.)
    PadParamChanged {
        pad: u8,
        param: PadParam,
        value: Value,
    },

    /// A device-wide system parameter changed (identity, the firmware-update +
    /// download lifecycle, date/time + personalization settings, the global
    /// output disables, or USB / storage / sharing status), on the single root
    /// `SYSTEM` node. Key-less: there is exactly one SYSTEM node. `param` is the
    /// typed property identity ([`SystemParam`]); an un-typed property arrives as
    /// [`SystemParam::Other`]. The `powerOffRequest` readback surfaces here as
    /// [`SystemParam::PowerOffRequest`], distinct from the dedicated
    /// [`crate::Command::PowerOff`] write frame.
    SystemParamChanged {
        param: SystemParam,
        value: Value,
    },

    /// `mixLevelWithAnchor` carries two fields, `anchor|value`. `anchor` is the
    /// configured per-route matrix level; `value` is the live fader-tracked
    /// level (equal to `anchor` when the wire sends a single number).
    MixLevelChanged {
        source: Source,
        mix: MixOutput,
        anchor: f32,
        value: f32,
    },
    MixMuteChanged {
        source: Source,
        mix: MixOutput,
        muted: bool,
    },
    MixLinkChanged {
        source: Source,
        mix: MixOutput,
        linked: bool,
    },
    MixDisabledChanged {
        source: Source,
        mix: MixOutput,
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
    let model = layout.model();

    // CHANNEL-addressed properties (single-level path). A path that resolves to
    // a channel with no named fader (the master strip, index 9) falls through
    // to Unknown rather than matching here.
    if let Some(fader) = layout
        .channel_index_from_path(path)
        .and_then(|idx| Fader::from_index(model, idx))
    {
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
    if let Some(fader) = layout
        .fader_index_from_path(path)
        .and_then(|idx| Fader::from_index(model, idx))
    {
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
    if let Some((source, mix)) = layout
        .mix_cell_from_path(path)
        .and_then(|(s, m)| Some((Source::from_protocol(s)?, MixOutput::from_protocol(m)?)))
    {
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
                if let Some(fader) = Fader::from_index(model, raw as u8) {
                    return DeviceEvent::FaderTouched { fader };
                }
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
            .filter(|&idx| idx < layout.channel_count() as u32)
            .and_then(|idx| Fader::from_index(model, idx as u8))
        {
            let source = value
                .as_ref()
                .and_then(Value::as_int)
                .filter(|&s| s >= 0)
                .and_then(|s| u8::try_from(s).ok())
                .and_then(Source::from_protocol);
            return DeviceEvent::FaderAssignmentChanged { fader, source };
        }
    }

    // Typed param-family promotion: any *other* property on an addressable node
    // becomes the matching typed `*ParamChanged` event, carrying the
    // self-describing wire value. `resolve_param_target` decides which family the
    // path lands on (CHANNEL strip, INPUTSOURCE, the MASTERCHANNEL / OUTPUT /
    // DUCKER / RECORDER / PLAYER singletons, or a HEADPHONE jack). Those node
    // index ranges are disjoint, so the resolver order is for clarity, not
    // precedence. `value` is moved exactly once: into the matched event, else
    // into Unknown (which also catches `propertyRemoved`, where value is None).
    match (resolve_param_target(path, name, layout, model), value) {
        (Some(ParamTarget::Channel(fader)), Some(value)) => DeviceEvent::ChannelParamChanged {
            fader,
            param: ChannelParam::from_name(name),
            value,
        },
        (Some(ParamTarget::InputSource(source)), Some(value)) => {
            DeviceEvent::InputSourceParamChanged {
                source,
                param: InputSourceParam::from_name(name),
                value,
            }
        }
        (Some(ParamTarget::Master), Some(value)) => DeviceEvent::MasterParamChanged {
            param: MasterParam::from_name(name),
            value,
        },
        (Some(ParamTarget::Output), Some(value)) => DeviceEvent::OutputParamChanged {
            param: OutputParam::from_name(name),
            value,
        },
        (Some(ParamTarget::Ducker), Some(value)) => DeviceEvent::DuckerParamChanged {
            param: DuckerParam::from_name(name),
            value,
        },
        (Some(ParamTarget::Recorder), Some(value)) => DeviceEvent::RecorderParamChanged {
            param: RecorderParam::from_name(name),
            value,
        },
        (Some(ParamTarget::Player), Some(value)) => DeviceEvent::PlayerParamChanged {
            param: PlayerParam::from_name(name),
            value,
        },
        (Some(ParamTarget::Headphone(headphone)), Some(value)) => {
            DeviceEvent::HeadphoneParamChanged {
                headphone,
                param: HeadphoneParam::from_name(name),
                value,
            }
        }
        (Some(ParamTarget::Effects(effects)), Some(value)) => DeviceEvent::EffectsParamChanged {
            effects,
            param: EffectsParam::from_name(name),
            value,
        },
        (Some(ParamTarget::Gui), Some(value)) => DeviceEvent::GuiParamChanged {
            param: GuiParam::from_name(name),
            value,
        },
        (Some(ParamTarget::Pad(pad)), Some(value)) => DeviceEvent::PadParamChanged {
            pad,
            param: PadParam::from_name(name),
            value,
        },
        (Some(ParamTarget::System), Some(value)) => DeviceEvent::SystemParamChanged {
            param: SystemParam::from_name(name),
            value,
        },
        (_, value) => DeviceEvent::Unknown {
            prop_name: name.to_string(),
            path: path.to_vec(),
            value,
        },
    }
}

/// Which addressable node family a property path lands on. Resolved by
/// [`resolve_param_target`] and consumed by [`decode_property`] to promote a
/// property to the matching typed `*ParamChanged` event.
enum ParamTarget {
    Channel(Fader),
    InputSource(Source),
    Master,
    Output,
    Ducker,
    Recorder,
    Player,
    Headphone(u8),
    Effects(u8),
    Gui,
    Pad(u8),
    System,
}

/// Resolve a property path to its addressable node family, if any. The node
/// index ranges are disjoint, so the first match wins and the order is for
/// readability only. Returns `None` for paths that don't land on a typed family
/// (the caller then emits [`DeviceEvent::Unknown`]).
fn resolve_param_target(
    path: &[u32],
    name: &str,
    layout: &Layout,
    model: DeviceModel,
) -> Option<ParamTarget> {
    // CHANNEL strip (the whole EQ / dynamics / HPF / aphex / pan / tone / preamp
    // strip). The three specialized props that own dedicated variants are
    // excluded so a malformed one of those falls to Unknown rather than
    // masquerading as a generic strip param.
    if !matches!(
        name,
        "channelOutputMute" | "channelCueEnable" | "channelInputSource"
    ) {
        if let Some(fader) = layout
            .channel_index_from_path(path)
            .and_then(|idx| Fader::from_index(model, idx))
        {
            return Some(ParamTarget::Channel(fader));
        }
    }
    // INPUTSOURCE: the input* preamp/source params, on a node separate from
    // CHANNEL. An ordinal past the named vocabulary (a rcSync / streamer-x
    // placeholder source) doesn't map and falls through to Unknown.
    if let Some(source) = layout
        .input_source_index_from_path(path)
        .and_then(Source::from_protocol)
    {
        return Some(ParamTarget::InputSource(source));
    }
    // Singletons: exactly one node each, key-less.
    if layout.is_master_channel_path(path) {
        return Some(ParamTarget::Master);
    }
    if layout.is_output_path(path) {
        return Some(ParamTarget::Output);
    }
    if layout.is_ducker_path(path) {
        return Some(ParamTarget::Ducker);
    }
    if layout.is_recorder_path(path) {
        return Some(ParamTarget::Recorder);
    }
    if layout.is_player_path(path) {
        return Some(ParamTarget::Player);
    }
    // HEADPHONE: multi-instance, one node per physical jack.
    if let Some(headphone) = layout.headphone_index_from_path(path) {
        return Some(ParamTarget::Headphone(headphone));
    }
    // EFFECTS_PARAMETERS: multi-instance, one node per channel-strip effects slot.
    if let Some(effects) = layout.effects_index_from_path(path) {
        return Some(ParamTarget::Effects(effects));
    }
    // GUI: the single front-panel UI-state node.
    if layout.is_gui_path(path) {
        return Some(ParamTarget::Gui);
    }
    // PAD: multi-instance, one node per sound pad inside SOUNDPADS (two-level).
    if let Some(pad) = layout.pad_index_from_path(path) {
        return Some(ParamTarget::Pad(pad));
    }
    // SYSTEM: the single device-wide state node.
    if layout.is_system_path(path) {
        return Some(ParamTarget::System);
    }
    None
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
    let model = layout.model();

    // 1. PHYSICALINTERFACE -> FADER initial levels (physical strips only).
    if let Some(phys) = root.children.iter().find(|n| n.name == "PHYSICALINTERFACE") {
        let mut fader_idx: u8 = 0;
        for child in &phys.children {
            if child.name != "FADER" {
                continue;
            }
            if let (Some(fader), Some(level)) = (
                Fader::from_index(model, fader_idx),
                int_prop(child, "faderLevel"),
            ) {
                out.push(DeviceEvent::FaderLevelChanged {
                    fader,
                    level: level.clamp(0, 127) as u8,
                });
            }
            fader_idx = fader_idx.saturating_add(1);
        }
    }

    // 2. CHANNEL initial mute/cue (one per strip, including virtuals). The
    // master strip (index 9) has no named fader, so its properties are skipped.
    let mut channel_idx: u8 = 0;
    for child in &root.children {
        if child.name != "CHANNEL" {
            continue;
        }
        if let Some(fader) = Fader::from_index(model, channel_idx) {
            if let Some(muted) = bool_prop(child, "channelOutputMute") {
                out.push(DeviceEvent::FaderMuteChanged { fader, muted });
            }
            if let Some(enabled) = bool_prop(child, "channelCueEnable") {
                out.push(DeviceEvent::FaderCueChanged { fader, enabled });
            }
            // In a fullSync the assignment sits on the Nth CHANNEL positionally
            // (stride 1), unlike the stride-6 incremental echo.
            if let Some(source_i) = int_prop(child, "channelInputSource") {
                out.push(DeviceEvent::FaderAssignmentChanged {
                    fader,
                    source: if source_i < 0 {
                        None
                    } else {
                        u8::try_from(source_i).ok().and_then(Source::from_protocol)
                    },
                });
            }
            // Everything else on the CHANNEL node is the channel strip (EQ,
            // dynamics, HPF, aphex, pan, tone, preamp). Emit each as a typed,
            // fader-resolved param so the initial state carries the full strip,
            // not just mute/cue/source. Skips the three handled above.
            for p in &child.properties {
                if matches!(
                    p.name.as_str(),
                    "channelOutputMute" | "channelCueEnable" | "channelInputSource"
                ) {
                    continue;
                }
                out.push(DeviceEvent::ChannelParamChanged {
                    fader,
                    param: ChannelParam::from_name(&p.name),
                    value: p.value.clone(),
                });
            }
        }
        channel_idx = channel_idx.saturating_add(1);
    }

    // 3. MIX cells initial values. Source-major: cell N has source = N/13, mix
    // = N%13. Cells whose source ordinal falls past the named vocabulary (e.g.
    // a Duo's rcSync RCV placeholder block) don't map and are skipped; the
    // counter still advances so later cells keep their source-major position.
    let mut mix_counter: u32 = 0;
    let per_source = layout.mix_count_per_source() as u32;
    for child in &root.children {
        if child.name != "MIX" {
            continue;
        }
        let source_idx = (mix_counter / per_source) as u8;
        let mix_idx = (mix_counter % per_source) as u8;
        mix_counter += 1;
        let (source, mix) = match (
            Source::from_protocol(source_idx),
            MixOutput::from_protocol(mix_idx),
        ) {
            (Some(s), Some(m)) => (s, m),
            _ => continue,
        };
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
    }

    // 4. INPUTSOURCE initial preamp/source params (one set per addressable
    // source). The counter runs over every INPUTSOURCE node in tree order; the
    // ordinal == the device `inputId` == `Source::to_protocol`. Ordinals past
    // the named vocabulary (a Duo's rcSync / streamer-x placeholder block) don't
    // map and are skipped, the counter still advancing so later sources keep
    // their position.
    let mut input_source_idx: u32 = 0;
    for child in &root.children {
        if child.name != "INPUTSOURCE" {
            continue;
        }
        let ordinal = input_source_idx;
        input_source_idx += 1;
        let source = match u8::try_from(ordinal).ok().and_then(Source::from_protocol) {
            Some(s) => s,
            None => continue,
        };
        for p in &child.properties {
            out.push(DeviceEvent::InputSourceParamChanged {
                source,
                param: InputSourceParam::from_name(&p.name),
                value: p.value.clone(),
            });
        }
    }

    // 5. MASTERCHANNEL and OUTPUT initial params (singletons). Emit every
    // property the device carries on each node as a typed, key-less event.
    if let Some(node) = root.children.iter().find(|c| c.name == "MASTERCHANNEL") {
        for p in &node.properties {
            out.push(DeviceEvent::MasterParamChanged {
                param: MasterParam::from_name(&p.name),
                value: p.value.clone(),
            });
        }
    }
    if let Some(node) = root.children.iter().find(|c| c.name == "OUTPUT") {
        for p in &node.properties {
            out.push(DeviceEvent::OutputParamChanged {
                param: OutputParam::from_name(&p.name),
                value: p.value.clone(),
            });
        }
    }

    // 6. DUCKER / RECORDER / PLAYER initial params (singletons). Same key-less
    // shape: emit every property each node carries as its typed event.
    if let Some(node) = root.children.iter().find(|c| c.name == "DUCKER") {
        for p in &node.properties {
            out.push(DeviceEvent::DuckerParamChanged {
                param: DuckerParam::from_name(&p.name),
                value: p.value.clone(),
            });
        }
    }
    if let Some(node) = root.children.iter().find(|c| c.name == "RECORDER") {
        for p in &node.properties {
            out.push(DeviceEvent::RecorderParamChanged {
                param: RecorderParam::from_name(&p.name),
                value: p.value.clone(),
            });
        }
    }
    if let Some(node) = root.children.iter().find(|c| c.name == "PLAYER") {
        for p in &node.properties {
            out.push(DeviceEvent::PlayerParamChanged {
                param: PlayerParam::from_name(&p.name),
                value: p.value.clone(),
            });
        }
    }

    // 7. HEADPHONE initial params (multi-instance, one node per physical jack).
    // The counter runs over every HEADPHONE node in tree order; its ordinal is
    // the headphone index that events and commands address.
    let mut headphone_idx: u8 = 0;
    for child in &root.children {
        if child.name != "HEADPHONE" {
            continue;
        }
        let headphone = headphone_idx;
        headphone_idx = headphone_idx.saturating_add(1);
        for p in &child.properties {
            out.push(DeviceEvent::HeadphoneParamChanged {
                headphone,
                param: HeadphoneParam::from_name(&p.name),
                value: p.value.clone(),
            });
        }
    }

    // 8. EFFECTS_PARAMETERS initial params (multi-instance, one node per
    // channel-strip effects slot). Only the first contiguous run at root is the
    // addressable slots; the counter stops at the first non-EFFECTS_PARAMETERS
    // node so the nested PADEFFECTS set (a separate addressing context) is not
    // folded in. The ordinal is the slot index events and commands address.
    let mut effects_idx: u8 = 0;
    let mut seen_effects = false;
    for child in &root.children {
        if child.name != "EFFECTS_PARAMETERS" {
            // Stop at the end of the first run so nested-set siblings later in
            // the tree (if hoisted) never extend the addressable range.
            if seen_effects {
                break;
            }
            continue;
        }
        seen_effects = true;
        let effects = effects_idx;
        effects_idx = effects_idx.saturating_add(1);
        for p in &child.properties {
            out.push(DeviceEvent::EffectsParamChanged {
                effects,
                param: EffectsParam::from_name(&p.name),
                value: p.value.clone(),
            });
        }
    }

    // 9. GUI initial params (singleton). Same key-less shape as the other
    // singletons: emit every property the single front-panel UI-state node
    // carries as a typed GuiParamChanged.
    if let Some(node) = root.children.iter().find(|c| c.name == "GUI") {
        for p in &node.properties {
            out.push(DeviceEvent::GuiParamChanged {
                param: GuiParam::from_name(&p.name),
                value: p.value.clone(),
            });
        }
    }

    // 10. SOUNDPADS initial params (multi-instance PADs nested inside the
    // container). Only the first contiguous run of PAD nodes is the addressable
    // pads; the counter stops at the first non-PAD child so any sibling node
    // inside the container never extends the addressable range. The ordinal in
    // the run is the pad index events and commands address.
    if let Some(soundpads) = root.children.iter().find(|c| c.name == "SOUNDPADS") {
        let mut pad_idx: u8 = 0;
        let mut seen_pad = false;
        for child in &soundpads.children {
            if child.name != "PAD" {
                if seen_pad {
                    break;
                }
                continue;
            }
            seen_pad = true;
            let pad = pad_idx;
            pad_idx = pad_idx.saturating_add(1);
            for p in &child.properties {
                out.push(DeviceEvent::PadParamChanged {
                    pad,
                    param: PadParam::from_name(&p.name),
                    value: p.value.clone(),
                });
            }
        }
    }

    // 11. SYSTEM initial params (singleton). Same key-less shape as the other
    // singletons: emit every property the single device-wide state node carries
    // as a typed SystemParamChanged (identity, update lifecycle, date/time,
    // disables, USB/storage/sharing status). `boardType` surfaces here too, the
    // same value `Layout::detect_model` reads to pick the device model.
    if let Some(node) = root.children.iter().find(|c| c.name == "SYSTEM") {
        for p in &node.properties {
            out.push(DeviceEvent::SystemParamChanged {
                param: SystemParam::from_name(&p.name),
                value: p.value.clone(),
            });
        }
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
#[path = "events_tests.rs"]
mod tests;
