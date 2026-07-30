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
//!   (`0x1C`=fader0, `0x22`=fader1, ...). `decode_property` resolves the echo
//!   with that stride.
//! - `encoderSignal` ([`DeviceEvent::FaderTouched`]) and `encoderColour`
//!   ([`DeviceEvent::FaderEncoderColourChanged`]): single-level paths whose
//!   value is the raw fader index (no base offset).
//!
//! Both formulas reproduce the behaviour the reference server ran in
//! production; a future capture on newer firmware may refine them.

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
    /// Fader level (0..127 MIDI scale).
    ///
    /// **fw 1.7.3 reachability (Duo, 2026-06-30):** at runtime this event only
    /// fires for `Fader::Virtual*` strips. Pushing a `Fader::Physical*` strip
    /// emits no `faderLevel` property change on the JUCE wire; the hardware
    /// fader's live position only flows over MIDI CC#15 on UART3 to
    /// `rc_audio_mixer`, which echoes its effect downstream as a
    /// `mixLevelWithAnchor` sweep across the source's matrix column (each
    /// cell's `value` field carries the live fader position). Physical fader
    /// positions DO surface here once at initial state via
    /// [`extract_initial_state`] (the fullSync seeds them from
    /// `PHYSICALINTERFACE > FADER.faderLevel`), but the runtime change path is
    /// unreachable for physical strips on this firmware.
    FaderLevelChanged {
        fader: Fader,
        level: u8,
    },
    /// A fader strip was touched (the device's `encoderSignal`). The wire
    /// addresses it by raw fader index (single-level path, no base offset).
    ///
    /// **fw 1.7.3 reachability (Duo, 2026-06-30):** observed for `Virtual*`
    /// strips driven from the touchscreen. Pushing a `Physical*` strip emits
    /// only a `mixLevelWithAnchor` sweep on its source's matrix column and no
    /// `encoderSignal`; the hardware touch sensor's events appear to stay on
    /// UART3 to `rc_audio_mixer` rather than crossing the JUCE wire.
    FaderTouched {
        fader: Fader,
    },
    /// The LED-ring colour index of a fader strip's rotary encoder changed
    /// (the device's `encoderColour`). Same single-level addressing as
    /// `encoderSignal`: the path is the raw fader index. `colour` is `None`
    /// when the wire value is `-1` (cleared), otherwise the palette index the
    /// device emitted.
    ///
    /// Captured 2026-06-30 from a real Duo capture; palette index semantics
    /// (what each value paints) are not modeled here, only round-tripped.
    FaderEncoderColourChanged {
        fader: Fader,
        colour: Option<i32>,
    },
    /// A `mixLinkRequest` or `mixUnlinkRequest` property carrying a `Binary`
    /// payload was observed at a mix cell's single-level path. Captured
    /// 2026-06-30 against a Duo on fw 1.7.3.
    ///
    /// `direction` (Link vs Unlink) comes from the property name. `origin`
    /// (ClientTrigger vs DeviceAck) comes from the third byte of the payload
    /// (`0x02` = client-initiated trigger, `0x03` = device-emitted
    /// acknowledgment). The earlier "press / release" interpretation was
    /// modeling them as symmetric phases of a single gesture, but probes
    /// showed the two have asymmetric origins: only client triggers cause
    /// state changes; the ack is the device writing the property back into
    /// the same Binary slot after running the link state machine.
    ///
    /// State change confirmation comes via [`DeviceEvent::MixLinkChanged`];
    /// this event is the protocol-level trace of the request submission and
    /// acknowledgment, useful for replay-fidelity tooling and capture
    /// diffing. The remaining payload bytes are arbitrary — only the third
    /// byte gates the device's behaviour.
    MixLinkRequested {
        source: Source,
        mix: MixOutput,
        direction: MixLinkDirection,
        origin: MixLinkRequestOrigin,
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

    /// A device-wide networking parameter changed (WiFi, Bluetooth, wired IP,
    /// cellular, DNS) on the singleton `NETWORK` node. Un-typed properties
    /// arrive as [`NetworkParam::Other`] rather than falling through to
    /// [`DeviceEvent::Unknown`]. Note that WiFi PSK and MAC-address strings
    /// pass through byte-faithfully; consumers should treat them as sensitive.
    NetworkParamChanged {
        param: NetworkParam,
        value: Value,
    },

    /// A recordings-container parameter changed (`recordingTotalCount` /
    /// `recordingTotalDuration` / `requestDeleteUID`) on the singleton
    /// `RECORDINGS` node. This is the container's summary state; individual
    /// recording metadata surfaces as [`DeviceEvent::RecordingParamChanged`].
    RecordingsParamChanged {
        param: RecordingsParam,
        value: Value,
    },

    /// A per-recording parameter changed on one of the `RECORDING` child nodes
    /// under the `RECORDINGS` container. `recording` is the child index within
    /// the container (0..count); combined with the fullSync's recording list
    /// order, it identifies which recording. `Content` values are pipe-separated
    /// metadata strings (`name|hash|path|timestampMs|durationSec|flags...`);
    /// parsing the fields is the caller's responsibility.
    RecordingParamChanged {
        recording: u8,
        param: RecordingParam,
        value: Value,
    },

    /// A per-volume storage parameter changed on one of the `STORAGEVOLUME`
    /// child nodes. `volume` is the child index (0 = the SD card slot on the
    /// devices we've captured; additional volumes appear if USB / other
    /// storage is attached). `State` values are pipe-separated live progress
    /// strings (`totalBytes|usedBytes|f|f|f`); parsing is the caller's job.
    StorageVolumeParamChanged {
        volume: u8,
        param: StorageVolumeParam,
        value: Value,
    },

    /// A `POT` child of `PHYSICALINTERFACE` reported a new rotary encoder
    /// position. `pot` is the raw child index within `PHYSICALINTERFACE`
    /// (which pot that resolves to depends on the device model; the Duo has
    /// two pots). `level` is the wire value clamped to 0..=127 (MIDI-scale).
    PotLevelChanged {
        pot: u8,
        level: u8,
    },

    /// A `PADBUTTON` child of `PHYSICALINTERFACE` reported a press or release
    /// on one of the front-panel pad buttons. `button` is the raw child index
    /// within `PHYSICALINTERFACE`; per-device mapping to the physical button
    /// row is the caller's job (the Duo emits indices 35..40 for six pad
    /// buttons on fw 1.7.3).
    PadButtonPressed {
        button: u8,
        pressed: bool,
    },

    /// A `SOLOMUTEBUTTON` child of `PHYSICALINTERFACE` reported a press or
    /// release on one of the front-panel mute buttons. `button` is the raw
    /// child index within `PHYSICALINTERFACE`.
    MutePressed {
        button: u8,
        pressed: bool,
    },

    /// A device-wide audio-engine parameter changed on the singleton `AUDIO`
    /// node (buffer / sample rate / channel counts / latencies / rcSync /
    /// StreamerX preset). Un-typed properties arrive as [`AudioParam::Other`].
    AudioParamChanged {
        param: AudioParam,
        value: Value,
    },

    /// A firmware-build metadata field changed on the singleton `BUILD` node.
    /// Read-back only in normal operation; writes are firmware-side.
    BuildParamChanged {
        param: BuildParam,
        value: Value,
    },

    /// A companion-app-mode flag changed on the singleton `APP` node
    /// (compression, monitor mix, output device, recording).
    AppParamChanged {
        param: AppParam,
        value: Value,
    },

    /// The device-wide UI theme changed on the singleton `THEME` node.
    ThemeParamChanged {
        param: ThemeParam,
        value: Value,
    },

    /// The identity of the currently-loaded show changed on the singleton
    /// `CURRENTSHOW` node. See [`DeviceEvent::ShowParamChanged`] for per-show
    /// metadata under the `SHOWS` container.
    CurrentShowParamChanged {
        param: CurrentShowParam,
        value: Value,
    },

    /// A per-show metadata field changed on one of the `SHOW` child nodes
    /// under the `SHOWS` container. `show` is the child index within the
    /// container (0..count).
    ShowParamChanged {
        show: u8,
        param: ShowParam,
        value: Value,
    },

    /// A show-lifecycle command or progress field changed on the singleton
    /// `SHOWCONTROL` node (delete / export / import / new-from-default,
    /// plus progress + last error).
    ShowControlParamChanged {
        param: ShowControlParam,
        value: Value,
    },

    /// A live meter reading changed on one of the `METER` nodes (typically
    /// one per fader strip). `meter` is the raw meter index within the tree
    /// structure that owns the METER nodes.
    MeterParamChanged {
        meter: u8,
        param: MeterParam,
        value: Value,
    },

    /// An `ENCODER` child of `PHYSICALINTERFACE` reported a press / release
    /// on the rotary encoder button. `encoder` is the raw child index.
    EncoderPressed {
        encoder: u8,
        pressed: bool,
    },

    /// A `SOLOMUTEBUTTON` child of `PHYSICALINTERFACE` reported a solo press
    /// or release. Companion to [`DeviceEvent::MutePressed`] (same node type,
    /// different property).
    SoloPressed {
        button: u8,
        pressed: bool,
    },

    /// The `RECBUTTON` child of `PHYSICALINTERFACE` reported a press or
    /// release on the device's REC button.
    RecButtonPressed {
        pressed: bool,
    },

    /// The singleton `EMERGENCYMUTE` node's `emergencyMuteActive` flag flipped.
    EmergencyMuteChanged {
        active: bool,
    },

    /// A SIP calling-level parameter changed on the singleton `SIPCALLING`
    /// node. Covers hosting flags, invite code, call-setup channels,
    /// subscription meters, post-call rating.
    SipCallingParamChanged {
        param: SipCallingParam,
        value: Value,
    },

    /// A per-registration SIP parameter changed on one of the
    /// `SIPREGISTRATION` child nodes under `SIPCALLING`. `registration` is
    /// the child ordinal (Duo carries two slots).
    SipRegistrationParamChanged {
        registration: u8,
        param: SipRegistrationParam,
        value: Value,
    },

    /// A per-call-slot SIP parameter changed on one of the `SIPCALLSLOTS`
    /// nodes. `slot` is the ordinal within the discovered run (Duo carries
    /// three slots). Statistics fields (Quality / Jitter / Bitrate /
    /// PacketLoss) are device-managed read-back.
    SipCallSlotsParamChanged {
        slot: u8,
        param: SipCallSlotsParam,
        value: Value,
    },

    /// A SIP advanced-settings parameter changed on the singleton
    /// `SIPADVANCED` node. Every property in this family is writable +
    /// persistent on Duo fw 1.7.3; writes to registration-relevant fields
    /// trigger a `SipRegistrationParamChanged { IsRegistered }` echo as the
    /// device re-checks its registration state.
    SipAdvancedParamChanged {
        param: SipAdvancedParam,
        value: Value,
    },

    /// A per-preset StreamerX mix-preset parameter changed. `preset` is the
    /// discovered ordinal of the `STREAMERXMIXPRESET` node.
    StreamerXMixPresetParamChanged {
        preset: u8,
        param: StreamerXMixPresetParam,
        value: Value,
    },

    /// A per-stream StreamerX mix-level parameter changed. `stream` is the
    /// discovered ordinal of the `STREAMERXSTREAMMIX` node.
    StreamerXStreamMixParamChanged {
        stream: u8,
        param: StreamerXStreamMixParam,
        value: Value,
    },

    /// A per-preset effects parameter changed on one of the `FXPRESET`
    /// nodes. `preset` is the discovered ordinal.
    FxPresetParamChanged {
        preset: u8,
        param: FxPresetParam,
        value: Value,
    },

    /// A per-pad-recorder parameter changed on one of the `PADRECORDER`
    /// nodes. `pad_recorder` is the discovered ordinal.
    PadRecorderParamChanged {
        pad_recorder: u8,
        param: PadRecorderParam,
        value: Value,
    },

    /// A device-diagnostic parameter changed on the singleton `TEST` node
    /// (factory-test LED all-white toggle, internal tone generator).
    TestParamChanged {
        param: TestParam,
        value: Value,
    },

    /// A per-scan-result WiFi SSID appeared on one of the `WIFISCANRESULT`
    /// nodes. `slot` is the discovered ordinal (the device populates a
    /// contiguous run as scans complete).
    WifiScanResultChanged {
        slot: u8,
        param: WifiScanResultParam,
        value: Value,
    },

    /// A wireless-radio pairing-lifecycle parameter changed on the singleton
    /// `RADIO` node.
    RadioParamChanged {
        param: RadioParam,
        value: Value,
    },

    /// A per-transmitter wireless-radio parameter changed on one of the
    /// `RADIOTX` nodes. `tx` is the discovered ordinal.
    RadioTxParamChanged {
        tx: u8,
        param: RadioTxParam,
        value: Value,
    },

    /// A per-receiver wireless-radio parameter changed on one of the
    /// `RADIORX` nodes. `rx` is the discovered ordinal.
    RadioRxParamChanged {
        rx: u8,
        param: RadioRxParam,
        value: Value,
    },

    /// A mix-minus routing parameter changed on either a `MIXMINUSES` or
    /// `RCSYNCMIXMINUES` node. The two node types share the same wire
    /// property name (`outputMixMinus`); consumers who need to distinguish
    /// must inspect the path context.
    MixMinusesParamChanged {
        param: MixMinusesParam,
        value: Value,
    },

    /// A per-rcSync-mix parameter changed on one of the `RCSYNCMIX` nodes.
    /// Six of the seven property names are shared with the regular MIX
    /// cell family; path shape distinguishes them at decode time (regular
    /// MIX cells fall inside the discovered mix run, RCSYNCMIX sits
    /// outside).
    RcSyncMixParamChanged {
        param: RcSyncMixParam,
        value: Value,
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

/// Direction of a mix-cell link request echo (see
/// [`DeviceEvent::MixLinkRequested`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MixLinkDirection {
    /// The device echoed a `mixLinkRequest` property (link action).
    Link,
    /// The device echoed a `mixUnlinkRequest` property (unlink action).
    Unlink,
}

/// Origin of a mix-cell link request observation (see
/// [`DeviceEvent::MixLinkRequested`]). Distinguishes the client-initiated
/// trigger from the device's own acknowledgment write, identified by the
/// third byte of the `Binary` payload.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MixLinkRequestOrigin {
    /// `byte[2] = 0x02`. A client wrote the trigger; the device runs the
    /// link state machine in response. This is what [`crate::Command::LinkMix`]
    /// and [`crate::Command::UnlinkMix`] emit.
    ClientTrigger,
    /// `byte[2] = 0x03`. The device wrote the property back to itself as an
    /// acknowledgment after running the link state machine. Not a frame any
    /// client should emit; the device ignores `Binary` writes with this
    /// pattern.
    DeviceAck,
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
            // Direction = property name; origin = byte[2] of the 6-byte Binary
            // payload (0x02 = client trigger, 0x03 = device ack). See
            // `DeviceEvent::MixLinkRequested` and the doc on `Command::LinkMix`.
            "mixLinkRequest" | "mixUnlinkRequest" => {
                if let Some(origin) = value.as_ref().and_then(mix_link_request_origin) {
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
    if NetworkParam::from_name(name).is_known() {
        if let Some(value) = value {
            return DeviceEvent::NetworkParamChanged {
                param: NetworkParam::from_name(name),
                value,
            };
        }
    }

    // RECORDINGS singleton (path.len() == 1).
    if path.len() == 1 && RecordingsParam::from_name(name).is_known() {
        if let Some(value) = value {
            return DeviceEvent::RecordingsParamChanged {
                param: RecordingsParam::from_name(name),
                value,
            };
        }
    }

    // RECORDING children (path.len() == 2). path[1] is the child index within
    // the RECORDINGS container.
    if path.len() == 2 && RecordingParam::from_name(name).is_known() {
        if let Some(value) = value {
            return DeviceEvent::RecordingParamChanged {
                recording: path[1] as u8,
                param: RecordingParam::from_name(name),
                value,
            };
        }
    }

    // STORAGEVOLUME children (path.len() == 2). path[1] is the volume index.
    if path.len() == 2 && StorageVolumeParam::from_name(name).is_known() {
        if let Some(value) = value {
            return DeviceEvent::StorageVolumeParamChanged {
                volume: path[1] as u8,
                param: StorageVolumeParam::from_name(name),
                value,
            };
        }
    }

    // AUDIO singleton (path.len() == 1). Property names are all `audio*`,
    // `activeStreamerX*`, `rcSync*` — unique to this node.
    if path.len() == 1 && AudioParam::from_name(name).is_known() {
        if let Some(value) = value {
            return DeviceEvent::AudioParamChanged {
                param: AudioParam::from_name(name),
                value,
            };
        }
    }

    // BUILD singleton — property names all start with `build*`.
    if path.len() == 1 && BuildParam::from_name(name).is_known() {
        if let Some(value) = value {
            return DeviceEvent::BuildParamChanged {
                param: BuildParam::from_name(name),
                value,
            };
        }
    }

    // APP singleton — property names all start with `app*`.
    if path.len() == 1 && AppParam::from_name(name).is_known() {
        if let Some(value) = value {
            return DeviceEvent::AppParamChanged {
                param: AppParam::from_name(name),
                value,
            };
        }
    }

    // THEME singleton — one property `themeId`.
    if path.len() == 1 && ThemeParam::from_name(name).is_known() {
        if let Some(value) = value {
            return DeviceEvent::ThemeParamChanged {
                param: ThemeParam::from_name(name),
                value,
            };
        }
    }

    // CURRENTSHOW singleton — property names all start with `currentShow*`.
    if path.len() == 1 && CurrentShowParam::from_name(name).is_known() {
        if let Some(value) = value {
            return DeviceEvent::CurrentShowParamChanged {
                param: CurrentShowParam::from_name(name),
                value,
            };
        }
    }

    // SHOWCONTROL singleton — property names all start with `showControl*`.
    if path.len() == 1 && ShowControlParam::from_name(name).is_known() {
        if let Some(value) = value {
            return DeviceEvent::ShowControlParamChanged {
                param: ShowControlParam::from_name(name),
                value,
            };
        }
    }

    // SHOW children under SHOWS container (path.len() == 2). path[1] is the
    // show index. Note: SHOW's property names (`showIcon`/`showName`/...)
    // don't clash with SHOWCONTROL's (`showControl*`) or CURRENTSHOW's
    // (`currentShow*`), so the name-based match is unambiguous.
    if path.len() == 2 && ShowParam::from_name(name).is_known() {
        if let Some(value) = value {
            return DeviceEvent::ShowParamChanged {
                show: path[1] as u8,
                param: ShowParam::from_name(name),
                value,
            };
        }
    }

    // METER children (path.len() == 2). Property names include the shared
    // `faderLevel`, but the path distinguishes: METER lives under a different
    // parent than FADER (which lives under PHYSICALINTERFACE and is handled
    // above), so this branch only fires for non-FADER-path `faderLevel` writes.
    if path.len() == 2
        && path[0] != layout.physical_interface_idx()
        && MeterParam::from_name(name).is_known()
    {
        if let Some(value) = value {
            return DeviceEvent::MeterParamChanged {
                meter: path[1] as u8,
                param: MeterParam::from_name(name),
                value,
            };
        }
    }

    // SIPCALLING singleton (path.len() == 1, layout-resolved position).
    if layout.is_sip_calling_path(path) && SipCallingParam::from_name(name).is_known() {
        if let Some(value) = value {
            return DeviceEvent::SipCallingParamChanged {
                param: SipCallingParam::from_name(name),
                value,
            };
        }
    }

    // SIPADVANCED singleton.
    if layout.is_sip_advanced_path(path) && SipAdvancedParam::from_name(name).is_known() {
        if let Some(value) = value {
            return DeviceEvent::SipAdvancedParamChanged {
                param: SipAdvancedParam::from_name(name),
                value,
            };
        }
    }

    // SIPREGISTRATION per-instance under SIPCALLING.
    if let Some(reg) = layout.sip_registration_index_from_path(path) {
        if SipRegistrationParam::from_name(name).is_known() {
            if let Some(value) = value {
                return DeviceEvent::SipRegistrationParamChanged {
                    registration: reg,
                    param: SipRegistrationParam::from_name(name),
                    value,
                };
            }
        }
    }

    // SIPCALLSLOTS per-slot at root.
    if let Some(slot) = layout.sip_call_slots_index_from_path(path) {
        if SipCallSlotsParam::from_name(name).is_known() {
            if let Some(value) = value {
                return DeviceEvent::SipCallSlotsParamChanged {
                    slot,
                    param: SipCallSlotsParam::from_name(name),
                    value,
                };
            }
        }
    }

    // Small-family decodes for property names that don't conflict with any
    // typed family above. Each name is unique to its family, so we key by
    // name; per-instance families take path.last() as the ordinal (no
    // layout resolution — the path itself carries enough context).

    if StreamerXMixPresetParam::from_name(name).is_known() {
        if let Some(value) = value {
            let preset = path.last().copied().unwrap_or(0) as u8;
            return DeviceEvent::StreamerXMixPresetParamChanged {
                preset,
                param: StreamerXMixPresetParam::from_name(name),
                value,
            };
        }
    }
    if StreamerXStreamMixParam::from_name(name).is_known() {
        if let Some(value) = value {
            let stream = path.last().copied().unwrap_or(0) as u8;
            return DeviceEvent::StreamerXStreamMixParamChanged {
                stream,
                param: StreamerXStreamMixParam::from_name(name),
                value,
            };
        }
    }
    if FxPresetParam::from_name(name).is_known() {
        if let Some(value) = value {
            let preset = path.last().copied().unwrap_or(0) as u8;
            return DeviceEvent::FxPresetParamChanged {
                preset,
                param: FxPresetParam::from_name(name),
                value,
            };
        }
    }
    if PadRecorderParam::from_name(name).is_known() {
        if let Some(value) = value {
            let pad_recorder = path.last().copied().unwrap_or(0) as u8;
            return DeviceEvent::PadRecorderParamChanged {
                pad_recorder,
                param: PadRecorderParam::from_name(name),
                value,
            };
        }
    }
    if TestParam::from_name(name).is_known() {
        if let Some(value) = value {
            return DeviceEvent::TestParamChanged {
                param: TestParam::from_name(name),
                value,
            };
        }
    }
    if WifiScanResultParam::from_name(name).is_known() {
        if let Some(value) = value {
            // Path shape is [network_root_idx, scan_slot]; slot is path.last().
            let slot = path.last().copied().unwrap_or(0) as u8;
            return DeviceEvent::WifiScanResultChanged {
                slot,
                param: WifiScanResultParam::from_name(name),
                value,
            };
        }
    }
    if RadioParam::from_name(name).is_known() {
        if let Some(value) = value {
            return DeviceEvent::RadioParamChanged {
                param: RadioParam::from_name(name),
                value,
            };
        }
    }
    if RadioTxParam::from_name(name).is_known() {
        if let Some(value) = value {
            let tx = path.last().copied().unwrap_or(0) as u8;
            return DeviceEvent::RadioTxParamChanged {
                tx,
                param: RadioTxParam::from_name(name),
                value,
            };
        }
    }
    if RadioRxParam::from_name(name).is_known() {
        if let Some(value) = value {
            let rx = path.last().copied().unwrap_or(0) as u8;
            return DeviceEvent::RadioRxParamChanged {
                rx,
                param: RadioRxParam::from_name(name),
                value,
            };
        }
    }
    if MixMinusesParam::from_name(name).is_known() {
        if let Some(value) = value {
            return DeviceEvent::MixMinusesParamChanged {
                param: MixMinusesParam::from_name(name),
                value,
            };
        }
    }
    // RcSyncMixParam shares six wire names with the regular MIX cell family
    // (mixDisabled / mixLevelWithAnchor / mixLink / mixLinkRequest / mixMute /
    // mixUnlinkRequest). The MIX matrix decode above matches paths inside the
    // discovered `first_mix + source*13 + mix` run; anything outside that run
    // that still carries these names lands here. The seventh property
    // (`mixRcSyncLevelRequest`) is unique to RCSYNCMIX.
    if RcSyncMixParam::from_name(name).is_known() {
        if let Some(value) = value {
            return DeviceEvent::RcSyncMixParamChanged {
                param: RcSyncMixParam::from_name(name),
                value,
            };
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

/// Read the origin out of a `mixLinkRequest` / `mixUnlinkRequest` payload.
/// The 6-byte blob's third byte is a JUCE bool marker — `0x02` (true) means
/// a client wrote the trigger, `0x03` (false) means the device wrote back
/// its acknowledgment. Anything else (or non-Binary) returns `None` and the
/// event falls through to [`DeviceEvent::Unknown`].
fn mix_link_request_origin(value: &Value) -> Option<MixLinkRequestOrigin> {
    let bytes = match value {
        Value::Binary(b) => b,
        _ => return None,
    };
    match bytes.get(2)? {
        0x02 => Some(MixLinkRequestOrigin::ClientTrigger),
        0x03 => Some(MixLinkRequestOrigin::DeviceAck),
        _ => None,
    }
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
