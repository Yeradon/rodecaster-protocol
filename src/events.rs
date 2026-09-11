//! Inbound typed event notifications from a RØDECaster console.
//!
//! [`DeviceEvent`] represents notifications emitted by the device in response
//! to hardware changes or user actions:
//!
//! - Physical or virtual fader movements.
//! - Mute and cue/solo button toggles.
//! - Rotary encoder adjustments and color updates.
//! - Input source assignment changes.
//! - Audio processing adjustments (EQ, dynamics, HPF, pan).
//! - Sub-mix changes and routing matrix links.
//!
//! # Handling Events
//!
//! When using [`crate::ProtocolSession`], events are yielded by
//! [`crate::ProtocolSession::ingest`]:
//!
//! ```rust
//! use rodecaster_protocol::{DeviceEvent, Fader};
//!
//! fn handle_event(event: DeviceEvent) {
//!     match event {
//!         DeviceEvent::FaderLevelChanged { fader, level } => {
//!             println!("{fader:?} level is now {level}");
//!         }
//!         DeviceEvent::FaderMuteChanged { fader, muted } => {
//!             println!("{fader:?} muted: {muted}");
//!         }
//!         _ => {}
//!     }
//! }
//! ```

use crate::change_frame::{decode as decode_frame, ChangeFrame};
use crate::juce_var::Value;
use crate::layout::Layout;
use crate::names::{
    AppParam, AudioParam, BuildParam, ChannelParam, CurrentShowParam, DeviceModel, DuckerParam,
    EffectsParam, Fader, FxPresetParam, GuiParam, HeadphoneParam, InputSourceParam, MasterParam,
    MeterParam, MixMinusesParam, MixOutput, NetworkParam, OutputParam, PadParam, PadRecorderParam,
    PlayerParam, RadioParam, RadioRxParam, RadioTxParam, RcSyncMixParam, RecorderParam,
    RecordingParam, RecordingsParam, ShowControlParam, ShowParam, SipAdvancedParam,
    SipCallSlotsParam, SipCallingParam, SipRegistrationParam, Source, StorageVolumeParam,
    StreamerXMixPresetParam, StreamerXStreamMixParam, SystemParam, TestParam, ThemeParam,
    WifiScanResultParam,
};
use crate::trigger::decode_phase;
pub use crate::trigger::TriggerPhase;
use crate::valuetree::Node;

/// A typed event emitted by a connected RØDECaster console.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum DeviceEvent {
    /// Full device state received during initialization or sync.
    InitialState(Vec<DeviceEvent>),

    /// Fader mute state changed.
    FaderMuteChanged { fader: Fader, muted: bool },
    /// Fader cue/solo state changed.
    FaderCueChanged { fader: Fader, enabled: bool },
    /// Fader level changed (0..127 MIDI scale).
    FaderLevelChanged { fader: Fader, level: u8 },
    /// A fader strip was touched.
    FaderTouched { fader: Fader },
    /// Rotary encoder LED-ring colour palette index changed (`None` when cleared).
    FaderEncoderColourChanged { fader: Fader, colour: Option<i32> },
    /// Mix link or unlink request or acknowledgment trace.
    MixLinkRequested {
        source: Source,
        mix: MixOutput,
        direction: MixLinkDirection,
        origin: MixLinkRequestOrigin,
    },
    /// A fader's input-source assignment changed (`None` when unassigned).
    FaderAssignmentChanged {
        fader: Fader,
        source: Option<Source>,
    },

    /// A channel-strip parameter changed (EQ, dynamics, HPF, pan, tone, preamp).
    ChannelParamChanged {
        fader: Fader,
        param: ChannelParam,
        value: Value,
    },

    /// An input-source parameter changed (preamp gain, phantom power, mic type).
    InputSourceParamChanged {
        source: Source,
        param: InputSourceParam,
        value: Value,
    },

    /// A master-bus parameter changed (Compellor compressor, master delay).
    MasterParamChanged { param: MasterParam, value: Value },

    /// An output-bus parameter changed (monitor/Bluetooth level, mute, multi-out).
    OutputParamChanged { param: OutputParam, value: Value },

    /// Auto-ducking parameters changed.
    DuckerParamChanged { param: DuckerParam, value: Value },

    /// Recorder transport state or property changed.
    RecorderParamChanged { param: RecorderParam, value: Value },

    /// Sound player property changed (playback state, file, fade envelope).
    PlayerParamChanged { param: PlayerParam, value: Value },

    /// Per-headphone parameter changed (colour, output type).
    HeadphoneParamChanged {
        headphone: u8,
        param: HeadphoneParam,
        value: Value,
    },

    /// Per-slot channel effects parameter changed (reverb, delay, pitch, distortion).
    EffectsParamChanged {
        effects: u8,
        param: EffectsParam,
        value: Value,
    },

    /// Front-panel UI parameter changed (display/button brightness, pad bank).
    GuiParamChanged { param: GuiParam, value: Value },

    /// Sound-pad parameter changed (colour, sample, playback mode, routing).
    PadParamChanged {
        pad: u8,
        param: PadParam,
        value: Value,
    },

    /// Device-wide system parameter changed (power, clock, USB/storage status).
    SystemParamChanged { param: SystemParam, value: Value },

    /// Routing matrix level changed (`anchor` is matrix level, `value` is live fader-tracked level).
    MixLevelChanged {
        source: Source,
        mix: MixOutput,
        anchor: f32,
        value: f32,
    },
    /// Routing matrix mute state changed.
    MixMuteChanged {
        source: Source,
        mix: MixOutput,
        muted: bool,
    },
    /// Routing matrix link state changed.
    MixLinkChanged {
        source: Source,
        mix: MixOutput,
        linked: bool,
    },
    /// Routing matrix route disabled state changed.
    MixDisabledChanged {
        source: Source,
        mix: MixOutput,
        disabled: bool,
    },

    /// Device-wide networking parameter changed (WiFi, Bluetooth, IP, DNS).
    NetworkParamChanged { param: NetworkParam, value: Value },

    /// Recordings summary parameter changed (total count, total duration).
    RecordingsParamChanged {
        param: RecordingsParam,
        value: Value,
    },

    /// Specific recording metadata changed.
    RecordingParamChanged {
        recording: u8,
        param: RecordingParam,
        value: Value,
    },

    /// Storage volume parameter changed (total bytes, used bytes).
    StorageVolumeParamChanged {
        volume: u8,
        param: StorageVolumeParam,
        value: Value,
    },

    /// Rotary encoder or potentiometer position changed (0..=127).
    PotLevelChanged { pot: u8, level: u8 },

    /// Front-panel pad button press or release.
    PadButtonPressed { button: u8, pressed: bool },

    /// Front-panel mute button press or release.
    MutePressed { button: u8, pressed: bool },

    /// Device-wide audio-engine parameter changed (sample rate, buffer size, latencies).
    AudioParamChanged { param: AudioParam, value: Value },

    /// Firmware build metadata changed.
    BuildParamChanged { param: BuildParam, value: Value },

    /// Companion-app state flag changed.
    AppParamChanged { param: AppParam, value: Value },

    /// UI theme setting changed.
    ThemeParamChanged { param: ThemeParam, value: Value },

    /// Currently-loaded show identifier changed.
    CurrentShowParamChanged {
        param: CurrentShowParam,
        value: Value,
    },

    /// Show metadata changed.
    ShowParamChanged {
        show: u8,
        param: ShowParam,
        value: Value,
    },

    /// Show control or lifecycle action changed.
    ShowControlParamChanged {
        param: ShowControlParam,
        value: Value,
    },

    /// Live level meter reading changed.
    MeterParamChanged {
        meter: u8,
        param: MeterParam,
        value: Value,
    },

    /// Rotary encoder push-button press or release.
    EncoderPressed { encoder: u8, pressed: bool },

    /// Front-panel solo/listen button press or release.
    SoloPressed { button: u8, pressed: bool },

    /// Front-panel record button press or release.
    RecButtonPressed { pressed: bool },

    /// Emergency mute state changed.
    EmergencyMuteChanged { active: bool },

    /// SIP calling state parameter changed.
    SipCallingParamChanged {
        param: SipCallingParam,
        value: Value,
    },

    /// SIP account registration parameter changed.
    SipRegistrationParamChanged {
        registration: u8,
        param: SipRegistrationParam,
        value: Value,
    },

    /// SIP call slot parameter changed.
    SipCallSlotsParamChanged {
        slot: u8,
        param: SipCallSlotsParam,
        value: Value,
    },

    /// SIP advanced setting changed.
    SipAdvancedParamChanged {
        param: SipAdvancedParam,
        value: Value,
    },

    /// Streamer X mix preset parameter changed.
    StreamerXMixPresetParamChanged {
        preset: u8,
        param: StreamerXMixPresetParam,
        value: Value,
    },

    /// Streamer X stream mix parameter changed.
    StreamerXStreamMixParamChanged {
        stream: u8,
        param: StreamerXStreamMixParam,
        value: Value,
    },

    /// Effects preset parameter changed.
    FxPresetParamChanged {
        preset: u8,
        param: FxPresetParam,
        value: Value,
    },

    /// Pad recorder parameter changed.
    PadRecorderParamChanged {
        pad_recorder: u8,
        param: PadRecorderParam,
        value: Value,
    },

    /// Diagnostic or factory-test parameter changed.
    TestParamChanged { param: TestParam, value: Value },

    /// WiFi scan result entry changed.
    WifiScanResultChanged {
        slot: u8,
        param: WifiScanResultParam,
        value: Value,
    },

    /// Wireless receiver lifecycle parameter changed.
    RadioParamChanged { param: RadioParam, value: Value },

    /// Wireless transmitter parameter changed.
    RadioTxParamChanged {
        tx: u8,
        param: RadioTxParam,
        value: Value,
    },

    /// Wireless receiver parameter changed.
    RadioRxParamChanged {
        rx: u8,
        param: RadioRxParam,
        value: Value,
    },

    /// Mix-minus routing parameter changed.
    MixMinusesParamChanged {
        minuses: u8,
        param: MixMinusesParam,
        value: Value,
    },

    /// rcSync mix routing parameter changed.
    RcSyncMixParamChanged {
        mix: u8,
        param: RcSyncMixParam,
        value: Value,
    },

    /// Tree topology changed (child added, removed, or moved); layout should be rebuilt.
    LayoutInvalidated,

    /// Unrecognized property name or path under the current layout.
    Unknown {
        prop_name: String,
        path: Vec<u32>,
        value: Option<Value>,
    },
}

/// Direction of a mix-cell link request echo.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MixLinkDirection {
    /// Link action (`mixLinkRequest`).
    Link,
    /// Unlink action (`mixUnlinkRequest`).
    Unlink,
}

/// Origin of a mix-cell link request observation (alias for [`TriggerPhase`]).
pub type MixLinkRequestOrigin = TriggerPhase;

/// Decode one wire payload (the change-frame, not including transport frame)
/// into a typed event. Returns `None` if the payload isn't a recognized JUCE
/// change-frame at all.
pub fn decode_event(payload: &[u8], layout: &Layout) -> Option<DeviceEvent> {
    let frame = decode_frame(payload)?;
    Some(decode_change_frame(frame, layout))
}

/// Decode an already-parsed [`ChangeFrame`] through the discovered [`Layout`]
/// into a typed [`DeviceEvent`].
pub fn decode_event_from_frame(frame: ChangeFrame, layout: &Layout) -> DeviceEvent {
    decode_change_frame(frame, layout)
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
            "channelInputSource" => {
                let source = value
                    .as_ref()
                    .and_then(Value::as_int)
                    .filter(|&s| s >= 0)
                    .and_then(|s| u8::try_from(s).ok())
                    .and_then(Source::from_protocol);
                return DeviceEvent::FaderAssignmentChanged { fader, source };
            }
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
            // mixLinkRequest / mixUnlinkRequest at a cell's single-level path.
            // Direction = property name; origin = momentary trigger state.
            "mixLinkRequest" | "mixUnlinkRequest" => {
                if let Some(origin) = value.as_ref().and_then(decode_phase) {
                    let direction = if name == "mixLinkRequest" {
                        MixLinkDirection::Link
                    } else {
                        MixLinkDirection::Unlink
                    };
                    return DeviceEvent::MixLinkRequested {
                        source,
                        mix,
                        direction,
                        origin,
                    };
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

    // encoderColour: LED-ring colour of a fader strip's rotary encoder. Same
    // single-level addressing as encoderSignal; the value is an Int where -1
    // means "cleared / no colour" and >= 0 is the palette index. A missing
    // or non-Int value falls through to Unknown so we don't conflate the two.
    if name == "encoderColour" {
        if let Some(&raw) = path.first() {
            if raw < layout.fader_count() as u32 {
                if let Some(fader) = Fader::from_index(model, raw as u8) {
                    if let Some(i) = value.as_ref().and_then(Value::as_int) {
                        let colour = if i < 0 { None } else { Some(i as i32) };
                        return DeviceEvent::FaderEncoderColourChanged { fader, colour };
                    }
                }
            }
        }
    }

    // channelInputSource echo: resolves via stride-1 channel_index_from_path first,
    // falling back to stride-6 echo addressing when node is offset-addressed.
    if name == "channelInputSource" {
        let fader_opt = layout
            .channel_index_from_path(path)
            .and_then(|idx| Fader::from_index(model, idx))
            .or_else(|| {
                path.first()
                    .and_then(|raw| raw.checked_sub(layout.first_channel()))
                    .filter(|&offset| offset % 6 == 0)
                    .map(|offset| offset / 6)
                    .filter(|&idx| idx < layout.channel_count() as u32)
                    .and_then(|idx| Fader::from_index(model, idx as u8))
            });

        if let Some(fader) = fader_opt {
            let source = value
                .as_ref()
                .and_then(Value::as_int)
                .filter(|&s| s >= 0)
                .and_then(|s| u8::try_from(s).ok())
                .and_then(Source::from_protocol);
            return DeviceEvent::FaderAssignmentChanged { fader, source };
        }
    }

    // PHYSICALINTERFACE hardware-child input events. Path shape is
    // `[physical_interface_idx, child_index]`. Each property name here is
    // unique to its child node type, so we key off the name and hand the
    // caller the raw child index (per-model mapping is the caller's job).
    if path.len() == 2 && path[0] == layout.physical_interface_idx() {
        let child = path[1] as u8;
        match name {
            "potLevel" => {
                if let Some(Value::Int(level)) = value.as_ref() {
                    return DeviceEvent::PotLevelChanged {
                        pot: child,
                        level: (*level).clamp(0, 127) as u8,
                    };
                }
            }
            "padButtonPressed" => {
                if let Some(Value::Bool(pressed)) = value.as_ref() {
                    return DeviceEvent::PadButtonPressed {
                        button: child,
                        pressed: *pressed,
                    };
                }
            }
            "mutePressed" => {
                if let Some(Value::Bool(pressed)) = value.as_ref() {
                    return DeviceEvent::MutePressed {
                        button: child,
                        pressed: *pressed,
                    };
                }
            }
            "soloPressed" => {
                if let Some(Value::Bool(pressed)) = value.as_ref() {
                    return DeviceEvent::SoloPressed {
                        button: child,
                        pressed: *pressed,
                    };
                }
            }
            "encoderPressed" => {
                if let Some(Value::Bool(pressed)) = value.as_ref() {
                    return DeviceEvent::EncoderPressed {
                        encoder: child,
                        pressed: *pressed,
                    };
                }
            }
            "recButtonPressed" => {
                if let Some(Value::Bool(pressed)) = value.as_ref() {
                    return DeviceEvent::RecButtonPressed { pressed: *pressed };
                }
            }
            _ => {}
        }
    }

    // EMERGENCYMUTE singleton at root (path.len() == 1).
    if path.len() == 1 && name == "emergencyMuteActive" {
        if let Some(Value::Bool(active)) = value.as_ref() {
            return DeviceEvent::EmergencyMuteChanged { active: *active };
        }
    }

    // NETWORK singleton: property names are all `bt*`, `wifi*`, `cell*`, etc.
    // The wire-name set is unique to the NETWORK node, so we match on name
    // without checking path (any structurally-valid propertyChanged carrying
    // one of these names lands here).
    if let Some(param) = NetworkParam::from_known_name(name) {
        if let Some(value) = value {
            return DeviceEvent::NetworkParamChanged { param, value };
        }
    }

    // RECORDINGS singleton (path.len() == 1).
    if path.len() == 1 {
        if let Some(param) = RecordingsParam::from_known_name(name) {
            if let Some(value) = value {
                return DeviceEvent::RecordingsParamChanged { param, value };
            }
        }
    }

    // RECORDING children (path.len() == 2). path[1] is the child index within
    // the RECORDINGS container.
    if path.len() == 2 {
        if let Some(param) = RecordingParam::from_known_name(name) {
            if let Some(value) = value {
                return DeviceEvent::RecordingParamChanged {
                    recording: path[1] as u8,
                    param,
                    value,
                };
            }
        }

        // STORAGEVOLUME children (path.len() == 2). path[1] is the volume index.
        if let Some(param) = StorageVolumeParam::from_known_name(name) {
            if let Some(value) = value {
                return DeviceEvent::StorageVolumeParamChanged {
                    volume: path[1] as u8,
                    param,
                    value,
                };
            }
        }
    }

    // Path-length-1 singletons: property names are unique to each family.
    if path.len() == 1 {
        if let Some(param) = AudioParam::from_known_name(name) {
            if let Some(value) = value {
                return DeviceEvent::AudioParamChanged { param, value };
            }
        }
        if let Some(param) = BuildParam::from_known_name(name) {
            if let Some(value) = value {
                return DeviceEvent::BuildParamChanged { param, value };
            }
        }
        if let Some(param) = AppParam::from_known_name(name) {
            if let Some(value) = value {
                return DeviceEvent::AppParamChanged { param, value };
            }
        }
        if let Some(param) = ThemeParam::from_known_name(name) {
            if let Some(value) = value {
                return DeviceEvent::ThemeParamChanged { param, value };
            }
        }
        if let Some(param) = CurrentShowParam::from_known_name(name) {
            if let Some(value) = value {
                return DeviceEvent::CurrentShowParamChanged { param, value };
            }
        }
        if let Some(param) = ShowControlParam::from_known_name(name) {
            if let Some(value) = value {
                return DeviceEvent::ShowControlParamChanged { param, value };
            }
        }
    }

    // Path-length-2 children under SHOWS and METER containers.
    if path.len() == 2 {
        if let Some(param) = ShowParam::from_known_name(name) {
            if let Some(value) = value {
                return DeviceEvent::ShowParamChanged {
                    show: path[1] as u8,
                    param,
                    value,
                };
            }
        }

        if path[0] != layout.physical_interface_idx() {
            if let Some(param) = MeterParam::from_known_name(name) {
                if let Some(value) = value {
                    return DeviceEvent::MeterParamChanged {
                        meter: path[1] as u8,
                        param,
                        value,
                    };
                }
            }
        }
    }

    // SIPCALLING singleton (path.len() == 1, layout-resolved position).
    if layout.is_sip_calling_path(path) {
        if let Some(param) = SipCallingParam::from_known_name(name) {
            if let Some(value) = value {
                return DeviceEvent::SipCallingParamChanged { param, value };
            }
        }
    }

    // SIPADVANCED singleton.
    if layout.is_sip_advanced_path(path) {
        if let Some(param) = SipAdvancedParam::from_known_name(name) {
            if let Some(value) = value {
                return DeviceEvent::SipAdvancedParamChanged { param, value };
            }
        }
    }

    // SIPREGISTRATION per-instance under SIPCALLING.
    if let Some(reg) = layout.sip_registration_index_from_path(path) {
        if let Some(param) = SipRegistrationParam::from_known_name(name) {
            if let Some(value) = value {
                return DeviceEvent::SipRegistrationParamChanged {
                    registration: reg,
                    param,
                    value,
                };
            }
        }
    }

    // SIPCALLSLOTS per-slot at root.
    if let Some(slot) = layout.sip_call_slots_index_from_path(path) {
        if let Some(param) = SipCallSlotsParam::from_known_name(name) {
            if let Some(value) = value {
                return DeviceEvent::SipCallSlotsParamChanged { slot, param, value };
            }
        }
    }

    // Small-family decodes for property names that don't conflict with any
    // typed family above. Each name is unique to its family, so we key by
    // name; per-instance families take path.last() as the ordinal (no
    // layout resolution: the path itself carries enough context).

    if let Some(param) = StreamerXMixPresetParam::from_known_name(name) {
        if let Some(value) = value {
            let preset = path.last().copied().unwrap_or(0) as u8;
            return DeviceEvent::StreamerXMixPresetParamChanged {
                preset,
                param,
                value,
            };
        }
    }
    if let Some(param) = StreamerXStreamMixParam::from_known_name(name) {
        if let Some(value) = value {
            let stream = path.last().copied().unwrap_or(0) as u8;
            return DeviceEvent::StreamerXStreamMixParamChanged {
                stream,
                param,
                value,
            };
        }
    }
    if let Some(param) = FxPresetParam::from_known_name(name) {
        if let Some(value) = value {
            let preset = path.last().copied().unwrap_or(0) as u8;
            return DeviceEvent::FxPresetParamChanged {
                preset,
                param,
                value,
            };
        }
    }
    if let Some(param) = PadRecorderParam::from_known_name(name) {
        if let Some(value) = value {
            let pad_recorder = path.last().copied().unwrap_or(0) as u8;
            return DeviceEvent::PadRecorderParamChanged {
                pad_recorder,
                param,
                value,
            };
        }
    }
    if let Some(param) = TestParam::from_known_name(name) {
        if let Some(value) = value {
            return DeviceEvent::TestParamChanged { param, value };
        }
    }
    if let Some(param) = WifiScanResultParam::from_known_name(name) {
        if let Some(value) = value {
            // Path shape is [network_root_idx, scan_slot]; slot is path.last().
            let slot = path.last().copied().unwrap_or(0) as u8;
            return DeviceEvent::WifiScanResultChanged { slot, param, value };
        }
    }
    if let Some(param) = RadioParam::from_known_name(name) {
        if let Some(value) = value {
            return DeviceEvent::RadioParamChanged { param, value };
        }
    }
    if let Some(param) = RadioTxParam::from_known_name(name) {
        if let Some(value) = value {
            let tx = path.last().copied().unwrap_or(0) as u8;
            return DeviceEvent::RadioTxParamChanged { tx, param, value };
        }
    }
    if let Some(param) = RadioRxParam::from_known_name(name) {
        if let Some(value) = value {
            let rx = path.last().copied().unwrap_or(0) as u8;
            return DeviceEvent::RadioRxParamChanged { rx, param, value };
        }
    }
    if let Some(minuses) = layout.mix_minuses_index_from_path(path) {
        if let Some(param) = MixMinusesParam::from_known_name(name) {
            if let Some(value) = value {
                return DeviceEvent::MixMinusesParamChanged {
                    minuses,
                    param,
                    value,
                };
            }
        }
    }
    // RcSyncMixParam shares six wire names with the regular MIX cell family
    // (mixDisabled / mixLevelWithAnchor / mixLink / mixLinkRequest / mixMute /
    // mixUnlinkRequest). The MIX matrix decode above matches paths inside the
    // discovered `first_mix + source*13 + mix` run; anything outside that run
    // that still carries these names lands here. The seventh property
    // (`mixRcSyncLevelRequest`) is unique to RCSYNCMIX.
    if let Some(mix) = layout.rcsync_mix_index_from_path(path) {
        if let Some(param) = RcSyncMixParam::from_known_name(name) {
            if let Some(value) = value {
                return DeviceEvent::RcSyncMixParamChanged { mix, param, value };
            }
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

/// Walk a parsed fullSync tree and extract the initial device state as a list of [`DeviceEvent`]s.
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
