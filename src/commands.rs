//! Typed Rodecaster commands.
//!
//! `Command` is the outgoing vocabulary the server (or any consumer) builds.
//! [`Command::encode`] produces one or more JUCE change-frame payloads,
//! addressed through a [`crate::Layout`] discovered from the device's
//! fullSync. The encoder routes every command through
//! [`crate::change_frame::encode_property_changed`], so the wire format is
//! always JUCE-faithful and the ID math lives in one place (the `Layout`),
//! never duplicated across files.
//!
//! Wrap each returned payload in a [`crate::frame::Packet`] for the transport.

use crate::change_frame;
use crate::juce_var::Value;
use crate::layout::Layout;
use crate::names::{
    AppParam, AudioParam, BuildParam, ChannelParam, CurrentShowParam, DeviceModel, DuckerParam,
    EffectsParam, Fader, FxPresetParam, GuiParam, HeadphoneParam, InputSourceParam, MasterParam,
    MixMinusesParam, MixOutput, NetworkParam, OutputParam, PadParam, PadRecorderParam, PlayerParam,
    RadioParam, RadioRxParam, RadioTxParam, RcSyncMixParam, RecorderParam, RecordingParam,
    RecordingsParam, ShowControlParam, ShowParam, SipAdvancedParam, SipCallSlotsParam,
    SipCallingParam, SipRegistrationParam, Source, StorageVolumeParam, StreamerXMixPresetParam,
    StreamerXStreamMixParam, SystemParam, TestParam, ThemeParam, WifiScanResultParam,
};

/// The single 6-byte trigger payload the device requires on `mixLinkRequest`
/// (to link) or `mixUnlinkRequest` (to unlink). Captured against a Duo on fw
/// 1.7.3 over USB HID, 2026-06-30, by isolating each variable from the wider
/// touchscreen behaviour.
///
/// **What the wire actually does:**
///
/// - `mixLinkRequest` and `mixUnlinkRequest` are properties whose natural
///   at-rest type is `Bool(false)`. The device's request handler ignores
///   `Bool` writes entirely and ignores `Binary` writes whose third byte is
///   `0x03` (the JUCE `false` marker).
/// - A `Binary` write whose third byte is `0x02` (the JUCE `true` marker)
///   is treated as a request submission: the device runs the link state
///   machine (flips `mixLink`, broadcasting `MixLinkChanged`) and then
///   writes the property back to a `Binary` value with `byte[2] = 0x03`
///   as a *device-side* acknowledgment.
/// - The remaining bytes of the payload are arbitrary; only `byte[2]`
///   gates the action. The touchscreen happens to fill the rest with
///   another `01,01,02` triplet (matching this constant byte-for-byte).
/// - Action direction is signalled entirely by the **property name**, not
///   the payload bytes.
///
/// What looked like a touchscreen "press/release" two-frame pulse was
/// actually one client-initiated trigger plus one device-initiated ack
/// written back into the same property. See [`crate::DeviceEvent::MixLinkRequested`]
/// for the inbound side ([`crate::MixLinkRequestOrigin::ClientTrigger`] vs
/// [`crate::MixLinkRequestOrigin::DeviceAck`]).
const MIX_LINK_TRIGGER: [u8; 6] = [0x01, 0x01, 0x02, 0x01, 0x01, 0x02];

/// CallMe return channels are toggled with a single legacy request blob (the
/// `(trigger=true, state=true)` form). CallMe sits outside the mix matrix on a
/// dedicated request path and has NOT been re-captured for the press/release
/// pulse, so it keeps the original single-frame behavior until validated.
const CALLME_REQUEST_BLOB: [u8; 6] = [0x01, 0x01, 0x02, 0x01, 0x01, 0x02];

/// JUCE `Int` value used to mean "unassigned" for `channelInputSource`.
/// The device treats negative source ids as "no source"; on the wire JUCE's
/// `INT` marker is a fixed 4-byte little-endian `i32`, so `-1` serializes as
/// `0xFFFF_FFFF` and decodes back to `-1`. The server emits the same bits
/// (its encoder uses `u32::MAX` which rolls over to the same `i32::-1`).
const CHANNEL_INPUT_SOURCE_UNASSIGNED: i64 = -1;

/// Verbatim wire bytes for the screen-wake message ([`Command::ScreenTouched`]).
///
/// This is NOT a well-formed `propertyChanged` frame and so cannot go through
/// [`change_frame::encode_property_changed`]: it is the `propertyChanged`
/// header (`changeType=1`, then `compressedInt(1)` for nLevels, then a path
/// num-bytes prefix `0x01`) followed *directly* by the property name with no
/// path value and no var value. The device special-cases it. Decoding it
/// through the generic codec would swallow the first name byte as the path
/// value, so it is emitted as a fixed literal. It is layout-independent (a
/// global "wake the display" request), so there is nothing to discover.
const SCREEN_TOUCHED_FRAME: [u8; 18] = [
    0x01, // changeType = PROPERTY_CHANGED
    0x01, 0x01, // compressedInt(1) = nLevels
    0x01, // path[0] num-bytes prefix (the name bytes follow with no value)
    b's', b'c', b'r', b'e', b'e', b'n', b'T', b'o', b'u', b'c', b'h', b'e', b'd', 0x00,
];

/// Root-child index that owns `powerOffRequest` on RODECaster Pro II firmware
/// 1.7.3 (empirically captured). Unlike the channel/mix/fader families, this
/// node is not part of a discoverable run in the fullSync, so it is pinned
/// here rather than derived from [`Layout`]. Revisit if a newer firmware (or
/// the Duo) addresses power-off differently.
const POWER_OFF_NODE_INDEX: u32 = 15;

/// Path offset for a CallMe routing request. CallMe return channels are
/// addressed *outside* the regular mix matrix (their sources sit past
/// `Layout::source_count`, so `mix_cell_path` cannot reach them). The device
/// instead accepts a dedicated single-level request path
/// `(source_index << 8) | (CALLME_MIX_PATH_OFFSET + mix)` carrying
/// `mixLinkRequest` / `mixUnlinkRequest`. Confirmed against firmware 1.7.3.
const CALLME_MIX_PATH_OFFSET: u32 = 4;

/// Outgoing Rodecaster command.
///
/// Each variant addresses a logical entity by *name* ([`Fader`], [`Source`],
/// [`MixOutput`]); [`Command::encode`] resolves the name to a wire index
/// through the [`Layout`] (and its [`DeviceModel`]), so this enum has zero
/// knowledge of `0x1C`, `+62`, or any other firmware-specific position
/// constant, and callers never pass a bare index.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum Command {
    /// Set the output mute on a fader strip.
    SetFaderMute { fader: Fader, mute: bool },
    /// Set the cue (pre-fader-listen) enable on a fader strip.
    SetFaderCue { fader: Fader, enable: bool },
    /// Set a virtual fader's level (0..127 MIDI scale).
    /// Physical fader levels come from hardware over MIDI/UART, not this path.
    SetFaderLevel { fader: Fader, level: u8 },
    /// Assign (or clear, with `None`) the input source for a fader strip.
    AssignFaderSource {
        fader: Fader,
        source: Option<Source>,
    },
    /// Enable or disable a routing matrix cell.
    SetMixDisabled {
        source: Source,
        mix: MixOutput,
        disabled: bool,
    },
    /// Mute or unmute a routing matrix cell. This is the per-cell `mixMute`,
    /// distinct from the per-strip [`Command::SetFaderMute`]. The device clears
    /// it as part of a native link; exposed on its own so callers can compose.
    SetMixMute {
        source: Source,
        mix: MixOutput,
        mute: bool,
    },
    /// Link a routing matrix cell with the device-validated minimal sequence:
    /// enable (`mixDisabled=false`), unmute (`mixMute=false`), then a single
    /// `Binary` trigger write to `mixLinkRequest`. `encode` returns three
    /// frames in order. The atomic halves are also callable on their own as
    /// [`Command::SetMixDisabled`] and [`Command::SetMixMute`]; a press on
    /// `mixLinkRequest` alone does NOT auto-clear `mixDisabled` or `mixMute`.
    LinkMix { source: Source, mix: MixOutput },
    /// Unlink a routing matrix cell with a single `Binary` trigger write to
    /// `mixUnlinkRequest`. `mixDisabled` and `mixMute` retain their prior
    /// values (the unlink does not re-disable or re-mute the cell).
    UnlinkMix { source: Source, mix: MixOutput },
    /// Wake the device display. A fixed, layout-independent message; see
    /// `SCREEN_TOUCHED_FRAME`.
    ScreenTouched,
    /// Request the device power off.
    PowerOff,
    /// Link a CallMe return channel into a mix. `source` must be a CallMe
    /// source ([`Source::CallMe1`]..=[`Source::CallMe3`]); on firmware where
    /// CallMe sits outside the mix matrix it is addressed by a dedicated
    /// request path, so this sends a single legacy `mixLinkRequest` blob (no
    /// enable/unmute or press/release pulse, unlike [`Command::LinkMix`]).
    LinkCallMe { source: Source, mix: MixOutput },
    /// Unlink a CallMe return channel from a mix. See [`Command::LinkCallMe`].
    UnlinkCallMe { source: Source, mix: MixOutput },
    /// Set any channel-strip DSP parameter (EQ, compressor, de-esser, noise
    /// gate, HPF, aphex, pan, tone, preamp) on a fader's CHANNEL node.
    ///
    /// This is the encode-side mirror of
    /// [`crate::DeviceEvent::ChannelParamChanged`]. The crate owns only the
    /// part it has verified on hardware: the CHANNEL *path* (resolved through
    /// [`Layout`]) and the property *name* (a firmware ground-truth string via
    /// [`ChannelParam`]). The `value` is **caller-supplied and its semantics
    /// are not all capture-verified** (range, scale, units differ per
    /// parameter), so the wire `Value` type and contents are the caller's
    /// responsibility. The JUCE wire is self-describing, so whatever `Value`
    /// you pass round-trips byte-faithfully; getting the device to *act* on it
    /// correctly is what needs a capture per parameter.
    SetChannelParam {
        fader: Fader,
        param: ChannelParam,
        value: Value,
    },
    /// Set any input-source parameter (preamp gain, 48V power, mic type, phase,
    /// colour, wireless serial, SIP/RCV routing) on a source's INPUTSOURCE node.
    ///
    /// The encode-side mirror of [`crate::DeviceEvent::InputSourceParamChanged`].
    /// Unlike [`Command::SetChannelParam`] this addresses the *source* directly
    /// (an `INPUTSOURCE` node, indexed by [`Source`]), independent of any fader
    /// assignment. The crate owns the INPUTSOURCE path (via [`Layout`]) and the
    /// property name (via [`InputSourceParam`]); the `value` is caller-supplied
    /// and rides byte-faithfully on the self-describing JUCE wire (per-parameter
    /// range/scale semantics are the caller's responsibility, as with
    /// [`Command::SetChannelParam`]).
    SetInputSourceParam {
        source: Source,
        param: InputSourceParam,
        value: Value,
    },
    /// Set a master-bus parameter (the master Compellor compressor or the master
    /// delay) on the single `MASTERCHANNEL` node.
    ///
    /// The encode-side mirror of [`crate::DeviceEvent::MasterParamChanged`].
    /// There is exactly one master bus, so this carries no addressing key. The
    /// crate owns the path (via [`Layout`]) and the property name (via
    /// [`MasterParam`]); the `value` is caller-supplied and rides byte-faithfully
    /// on the self-describing JUCE wire (per-parameter range/scale semantics are
    /// the caller's responsibility, as with [`Command::SetChannelParam`]).
    SetMasterParam { param: MasterParam, value: Value },
    /// Set an output-bus parameter (monitor/Bluetooth levels and mutes, the
    /// multi-out mode, or the recording-bus flags) on the single `OUTPUT` node.
    ///
    /// The encode-side mirror of [`crate::DeviceEvent::OutputParamChanged`].
    /// There is exactly one output bus, so this carries no addressing key. Path
    /// and property name are crate-owned (via [`Layout`] and [`OutputParam`]);
    /// the `value` is caller-supplied and rides byte-faithfully on the wire.
    SetOutputParam { param: OutputParam, value: Value },
    /// Set the auto-duck depth on the single `DUCKER` node.
    ///
    /// The encode-side mirror of [`crate::DeviceEvent::DuckerParamChanged`].
    /// Key-less: one ducker. Path and property name are crate-owned (via
    /// [`Layout`] and [`DuckerParam`]); the `value` rides byte-faithfully.
    SetDuckerParam { param: DuckerParam, value: Value },
    /// Set a recorder transport property on the single `RECORDER` node. The
    /// `request*` params are the actual command channel: set
    /// [`RecorderParam::RequestRecordState`] to start/stop recording, or
    /// [`RecorderParam::RequestDropMarker`] to drop a chapter marker.
    ///
    /// The encode-side mirror of [`crate::DeviceEvent::RecorderParamChanged`].
    /// Key-less: one recorder. Path and property name are crate-owned (via
    /// [`Layout`] and [`RecorderParam`]); the `value` rides byte-faithfully.
    SetRecorderParam { param: RecorderParam, value: Value },
    /// Set a long-form player property (transport, loaded file, or envelope) on
    /// the single `PLAYER` node.
    ///
    /// The encode-side mirror of [`crate::DeviceEvent::PlayerParamChanged`].
    /// Key-less: one player. Path and property name are crate-owned (via
    /// [`Layout`] and [`PlayerParam`]); the `value` rides byte-faithfully.
    SetPlayerParam { param: PlayerParam, value: Value },
    /// Set a per-headphone property (`headphoneColour` / `headphoneType`) on one
    /// `HEADPHONE` node, addressed by jack index.
    ///
    /// The encode-side mirror of [`crate::DeviceEvent::HeadphoneParamChanged`].
    /// Path (from the `headphone` index via [`Layout`]) and property name (via
    /// [`HeadphoneParam`]) are crate-owned; the `value` rides byte-faithfully.
    SetHeadphoneParam {
        headphone: u8,
        param: HeadphoneParam,
        value: Value,
    },
    /// Set a per-slot effects parameter (reverb, echo/delay, pitch shift,
    /// distortion, robot or voice-disguise control) on one root
    /// `EFFECTS_PARAMETERS` node, addressed by slot index.
    ///
    /// The encode-side mirror of [`crate::DeviceEvent::EffectsParamChanged`].
    /// Path (from the `effects` slot index via [`Layout`]) and property name (via
    /// [`EffectsParam`]) are crate-owned; the `value` rides byte-faithfully on the
    /// self-describing JUCE wire (per-parameter range/scale semantics are the
    /// caller's responsibility, as with [`Command::SetChannelParam`]).
    SetEffectsParam {
        effects: u8,
        param: EffectsParam,
        value: Value,
    },
    /// Set a front-panel UI parameter (display / button brightness, selected pad
    /// bank, metering mode, touchscreen EQ-band focus, ...) on the single root
    /// `GUI` node. Key-less: there is exactly one GUI node.
    ///
    /// The encode-side mirror of [`crate::DeviceEvent::GuiParamChanged`]. Path
    /// (the single `GUI` node via [`Layout`]) and property name (via [`GuiParam`])
    /// are crate-owned; the `value` rides byte-faithfully on the self-describing
    /// JUCE wire. This is ordinary UI state and is unrelated to
    /// [`Command::ScreenTouched`], which is a separate wake-the-display pulse.
    SetGuiParam { param: GuiParam, value: Value },
    /// Set a sound-pad parameter (colour / name / type / loaded sample /
    /// transport / gain / envelope / mixer routing / effect / SIP / MIDI-trigger
    /// control) on one `PAD` node inside the `SOUNDPADS` container, addressed by
    /// pad index.
    ///
    /// The encode-side mirror of [`crate::DeviceEvent::PadParamChanged`]. Path
    /// (from the `pad` index via [`Layout`]) and property name (via [`PadParam`])
    /// are crate-owned; the `value` rides byte-faithfully on the self-describing
    /// JUCE wire. `Err(EncodeError::OutOfRange)` if the pad index is past the
    /// discovered run.
    SetPadParam {
        pad: u8,
        param: PadParam,
        value: Value,
    },
    /// Set a device-wide system parameter (identity, the firmware-update +
    /// download command channel, date/time + personalization settings, the
    /// global output disables, or USB / storage / sharing status) on the single
    /// root `SYSTEM` node. Key-less: there is exactly one SYSTEM node.
    ///
    /// The encode-side mirror of [`crate::DeviceEvent::SystemParamChanged`]. Path
    /// (the single `SYSTEM` node via [`Layout`]) and property name (via
    /// [`SystemParam`]) are crate-owned; the `value` rides byte-faithfully on the
    /// self-describing JUCE wire (per-parameter range/scale semantics are the
    /// caller's responsibility, as with [`Command::SetChannelParam`]).
    ///
    /// Setting [`SystemParam::PowerOffRequest`] here addresses the *discovered*
    /// `SYSTEM` node, unlike the dedicated [`Command::PowerOff`] frame which is
    /// pinned to a fixed node index. On firmware 1.7.3 the discovered `SYSTEM`
    /// node is that same index, so the two are equivalent there; prefer
    /// [`Command::PowerOff`] for the plain "turn off" intent.
    SetSystemParam { param: SystemParam, value: Value },
    /// Set a SIP calling-level parameter on the singleton `SIPCALLING` node.
    /// Empirically verified writable on Duo fw 1.7.3 (2026-07-01): toggling
    /// `SipCallingParam::HostingEnabled` rotates `sipRodeCode` and re-registers.
    SetSipCallingParam {
        param: SipCallingParam,
        value: Value,
    },
    /// Set a per-registration SIP parameter on one of the SIPREGISTRATION
    /// child nodes under SIPCALLING. `registration` selects the slot
    /// (0..sip_registration_count).
    SetSipRegistrationParam {
        registration: u8,
        param: SipRegistrationParam,
        value: Value,
    },
    /// Set a per-call-slot SIP parameter on one of the SIPCALLSLOTS nodes.
    /// `slot` selects the slot (0..sip_call_slots_count). Statistics fields
    /// are device-managed and writes may be ignored; the crate accepts the
    /// write regardless.
    SetSipCallSlotsParam {
        slot: u8,
        param: SipCallSlotsParam,
        value: Value,
    },
    /// Set a SIP advanced-settings parameter on the singleton `SIPADVANCED`
    /// node. Empirically verified: all 25 typed properties in this family
    /// are writable + persistent on Duo fw 1.7.3. Writes to
    /// registration-relevant fields (account credentials, NAT, domain)
    /// trigger a device-side registration re-check echo.
    SetSipAdvancedParam {
        param: SipAdvancedParam,
        value: Value,
    },
    /// Set a diagnostic parameter on the singleton `TEST` node. Writing
    /// `TestParam::AllLedsWhite = Bool(true)` lights every front-panel LED
    /// (factory-test hook); `ToneGeneration = Int(N)` selects an internal
    /// test-tone source for audio-path verification.
    SetTestParam { param: TestParam, value: Value },
    /// Set a per-pad-recorder parameter on one of the `PADRECORDER` nodes.
    /// `pad_recorder` is the discovered ordinal (see
    /// [`Layout::pad_recorder_count`]). `StateRequest` is the write-side
    /// command channel to start / stop recording; `Clear` erases the pad's
    /// current recording.
    SetPadRecorderParam {
        pad_recorder: u8,
        param: PadRecorderParam,
        value: Value,
    },
    /// Set a per-preset effects parameter on one of the `FXPRESET` child
    /// nodes under `FXPRESETS`. `preset` is the discovered ordinal
    /// (see [`Layout::fx_preset_count`]). `Contents` is a serialized preset
    /// blob; `Idx` addresses the preset slot the contents apply to.
    SetFxPresetParam {
        preset: u8,
        param: FxPresetParam,
        value: Value,
    },
    /// Set a networking parameter on the singleton `NETWORK` node.
    SetNetworkParam { param: NetworkParam, value: Value },
    /// Set an audio engine parameter on the singleton `AUDIO` node.
    SetAudioParam { param: AudioParam, value: Value },
    /// Set a build parameter on the singleton `BUILD` node.
    SetBuildParam { param: BuildParam, value: Value },
    /// Set a companion-app parameter on the singleton `APP` node.
    SetAppParam { param: AppParam, value: Value },
    /// Set a theme parameter on the singleton `THEME` node.
    SetThemeParam { param: ThemeParam, value: Value },
    /// Set a current-show parameter on the singleton `CURRENTSHOW` node.
    SetCurrentShowParam {
        param: CurrentShowParam,
        value: Value,
    },
    /// Set a show-control parameter on the singleton `SHOWCONTROL` node.
    SetShowControlParam {
        param: ShowControlParam,
        value: Value,
    },
    /// Set a recordings-container parameter on the singleton `RECORDINGS` node.
    SetRecordingsParam {
        param: RecordingsParam,
        value: Value,
    },
    /// Set a wireless-radio parameter on the singleton `RADIO` node.
    SetRadioParam { param: RadioParam, value: Value },
    /// Set a per-show parameter on one of the `SHOW` child nodes under `SHOWS`.
    SetShowParam {
        show: u8,
        param: ShowParam,
        value: Value,
    },
    /// Set a per-recording parameter on one of the `RECORDING` child nodes under `RECORDINGS`.
    SetRecordingParam {
        recording: u8,
        param: RecordingParam,
        value: Value,
    },
    /// Set a storage volume parameter on one of the `STORAGEVOLUME` nodes.
    SetStorageVolumeParam {
        volume: u8,
        param: StorageVolumeParam,
        value: Value,
    },
    /// Set a wireless-radio transmitter parameter on one of the `RADIOTX` nodes.
    SetRadioTxParam {
        tx: u8,
        param: RadioTxParam,
        value: Value,
    },
    /// Set a wireless-radio receiver parameter on one of the `RADIORX` nodes.
    SetRadioRxParam {
        rx: u8,
        param: RadioRxParam,
        value: Value,
    },
    /// Set a WiFi scan result entry parameter on one of the `WIFISCANRESULT` nodes.
    SetWifiScanResultParam {
        slot: u8,
        param: WifiScanResultParam,
        value: Value,
    },
    /// Set a StreamerX mix preset parameter on one of the `STREAMERXMIXPRESET` nodes.
    SetStreamerXMixPresetParam {
        preset: u8,
        param: StreamerXMixPresetParam,
        value: Value,
    },
    /// Set a StreamerX stream mix parameter on one of the `STREAMERXSTREAMMIX` nodes.
    SetStreamerXStreamMixParam {
        stream: u8,
        param: StreamerXStreamMixParam,
        value: Value,
    },
    /// Set an rcSync mix parameter on one of the `RCSYNCMIX` nodes.
    SetRcSyncMixParam {
        mix: u8,
        param: RcSyncMixParam,
        value: Value,
    },
    /// Set a mix-minus parameter on one of the `MIXMINUSES` nodes.
    SetMixMinusesParam {
        minuses: u8,
        param: MixMinusesParam,
        value: Value,
    },
    /// Skip initial setup mode and configure language and timezone.
    SetupSkip {
        language: Option<String>,
        timezone: Option<String>,
    },
}

impl Command {
    /// Encode this command into one or more change-frame payloads to send in
    /// order. `Err(EncodeError::OutOfRange)` if the addressed entity does not
    /// exist in the layout (e.g., fader index past `Layout::fader_count`).
    ///
    /// Wrap each payload in a [`crate::frame::Packet`] for the transport.
    pub fn encode(&self, layout: &Layout) -> Result<Vec<Vec<u8>>, EncodeError> {
        match self {
            Command::SetFaderMute { fader, mute } => {
                let path = channel_path(layout, fader_index(layout, *fader)?)?;
                Ok(vec![change_frame::encode_property_changed(
                    &path,
                    "channelOutputMute",
                    &Value::Bool(*mute),
                )])
            }
            Command::SetFaderCue { fader, enable } => {
                let path = channel_path(layout, fader_index(layout, *fader)?)?;
                Ok(vec![change_frame::encode_property_changed(
                    &path,
                    "channelCueEnable",
                    &Value::Bool(*enable),
                )])
            }
            Command::SetFaderLevel { fader, level } => {
                let path = fader_path(layout, fader_index(layout, *fader)?)?;
                Ok(vec![change_frame::encode_property_changed(
                    &path,
                    "faderLevel",
                    &Value::Int(*level as i64),
                )])
            }
            Command::AssignFaderSource { fader, source } => {
                let path = channel_path(layout, fader_index(layout, *fader)?)?;
                let source_value = source
                    .map(|s| s.to_protocol() as i64)
                    .unwrap_or(CHANNEL_INPUT_SOURCE_UNASSIGNED);
                Ok(vec![change_frame::encode_property_changed(
                    &path,
                    "channelInputSource",
                    &Value::Int(source_value),
                )])
            }
            Command::SetMixDisabled {
                source,
                mix,
                disabled,
            } => {
                let path = mix_path(layout, *source, *mix)?;
                Ok(vec![change_frame::encode_property_changed(
                    &path,
                    "mixDisabled",
                    &Value::Bool(*disabled),
                )])
            }
            Command::SetMixMute { source, mix, mute } => {
                let path = mix_path(layout, *source, *mix)?;
                Ok(vec![change_frame::encode_property_changed(
                    &path,
                    "mixMute",
                    &Value::Bool(*mute),
                )])
            }
            Command::LinkMix { source, mix } => {
                let path = mix_path(layout, *source, *mix)?;
                // Link sequence: enable (the press alone does NOT auto-clear
                // mixDisabled), unmute (same for mixMute), then a single
                // Binary trigger write to mixLinkRequest. Empirically validated
                // on Duo fw 1.7.3 (2026-06-30) by isolating each variable.
                Ok(vec![
                    change_frame::encode_property_changed(
                        &path,
                        "mixDisabled",
                        &Value::Bool(false),
                    ),
                    change_frame::encode_property_changed(&path, "mixMute", &Value::Bool(false)),
                    change_frame::encode_property_changed(
                        &path,
                        "mixLinkRequest",
                        &Value::Binary(MIX_LINK_TRIGGER.to_vec()),
                    ),
                ])
            }
            Command::UnlinkMix { source, mix } => {
                let path = mix_path(layout, *source, *mix)?;
                // Single Binary trigger to mixUnlinkRequest is sufficient.
                // No enable/unmute precondition; the cell's other gates retain
                // whatever value they had.
                Ok(vec![change_frame::encode_property_changed(
                    &path,
                    "mixUnlinkRequest",
                    &Value::Binary(MIX_LINK_TRIGGER.to_vec()),
                )])
            }
            Command::ScreenTouched => Ok(vec![SCREEN_TOUCHED_FRAME.to_vec()]),
            Command::PowerOff => Ok(vec![change_frame::encode_property_changed(
                &[POWER_OFF_NODE_INDEX],
                "powerOffRequest",
                &Value::Bool(true),
            )]),
            Command::LinkCallMe { source, mix } => Ok(vec![change_frame::encode_property_changed(
                &callme_request_path(*source, *mix),
                "mixLinkRequest",
                &Value::Binary(CALLME_REQUEST_BLOB.to_vec()),
            )]),
            Command::UnlinkCallMe { source, mix } => {
                Ok(vec![change_frame::encode_property_changed(
                    &callme_request_path(*source, *mix),
                    "mixUnlinkRequest",
                    &Value::Binary(CALLME_REQUEST_BLOB.to_vec()),
                )])
            }
            Command::SetChannelParam {
                fader,
                param,
                value,
            } => {
                let path = channel_path(layout, fader_index(layout, *fader)?)?;
                Ok(vec![change_frame::encode_property_changed(
                    &path,
                    param.as_str(),
                    value,
                )])
            }
            Command::SetInputSourceParam {
                source,
                param,
                value,
            } => {
                let path = input_source_path(layout, *source)?;
                Ok(vec![change_frame::encode_property_changed(
                    &path,
                    param.as_str(),
                    value,
                )])
            }
            Command::SetMasterParam { param, value } => {
                let path = layout
                    .master_channel_path()
                    .ok_or(EncodeError::MissingNode {
                        what: "MASTERCHANNEL",
                    })?;
                Ok(vec![change_frame::encode_property_changed(
                    &path,
                    param.as_str(),
                    value,
                )])
            }
            Command::SetOutputParam { param, value } => {
                let path = layout
                    .output_path()
                    .ok_or(EncodeError::MissingNode { what: "OUTPUT" })?;
                Ok(vec![change_frame::encode_property_changed(
                    &path,
                    param.as_str(),
                    value,
                )])
            }
            Command::SetDuckerParam { param, value } => {
                let path = layout
                    .ducker_path()
                    .ok_or(EncodeError::MissingNode { what: "DUCKER" })?;
                Ok(vec![change_frame::encode_property_changed(
                    &path,
                    param.as_str(),
                    value,
                )])
            }
            Command::SetRecorderParam { param, value } => {
                let path = layout
                    .recorder_path()
                    .ok_or(EncodeError::MissingNode { what: "RECORDER" })?;
                Ok(vec![change_frame::encode_property_changed(
                    &path,
                    param.as_str(),
                    value,
                )])
            }
            Command::SetPlayerParam { param, value } => {
                let path = layout
                    .player_path()
                    .ok_or(EncodeError::MissingNode { what: "PLAYER" })?;
                Ok(vec![change_frame::encode_property_changed(
                    &path,
                    param.as_str(),
                    value,
                )])
            }
            Command::SetHeadphoneParam {
                headphone,
                param,
                value,
            } => {
                let path = headphone_path(layout, *headphone)?;
                Ok(vec![change_frame::encode_property_changed(
                    &path,
                    param.as_str(),
                    value,
                )])
            }
            Command::SetEffectsParam {
                effects,
                param,
                value,
            } => {
                let path = effects_path(layout, *effects)?;
                Ok(vec![change_frame::encode_property_changed(
                    &path,
                    param.as_str(),
                    value,
                )])
            }
            Command::SetGuiParam { param, value } => {
                let path = layout
                    .gui_path()
                    .ok_or(EncodeError::MissingNode { what: "GUI" })?;
                Ok(vec![change_frame::encode_property_changed(
                    &path,
                    param.as_str(),
                    value,
                )])
            }
            Command::SetPadParam { pad, param, value } => {
                let path = pad_path(layout, *pad)?;
                Ok(vec![change_frame::encode_property_changed(
                    &path,
                    param.as_str(),
                    value,
                )])
            }
            Command::SetSystemParam { param, value } => {
                let path = layout
                    .system_path()
                    .ok_or(EncodeError::MissingNode { what: "SYSTEM" })?;
                Ok(vec![change_frame::encode_property_changed(
                    &path,
                    param.as_str(),
                    value,
                )])
            }
            Command::SetSipCallingParam { param, value } => {
                let path = layout
                    .sip_calling_path()
                    .ok_or(EncodeError::MissingNode { what: "SIPCALLING" })?;
                Ok(vec![change_frame::encode_property_changed(
                    &path,
                    param.as_str(),
                    value,
                )])
            }
            Command::SetSipAdvancedParam { param, value } => {
                let path = layout.sip_advanced_path().ok_or(EncodeError::MissingNode {
                    what: "SIPADVANCED",
                })?;
                Ok(vec![change_frame::encode_property_changed(
                    &path,
                    param.as_str(),
                    value,
                )])
            }
            Command::SetSipRegistrationParam {
                registration,
                param,
                value,
            } => {
                let path = layout.sip_registration_path(*registration).ok_or(
                    EncodeError::MissingNode {
                        what: "SIPREGISTRATION",
                    },
                )?;
                Ok(vec![change_frame::encode_property_changed(
                    &path,
                    param.as_str(),
                    value,
                )])
            }
            Command::SetSipCallSlotsParam { slot, param, value } => {
                let path = layout
                    .sip_call_slots_path(*slot)
                    .ok_or(EncodeError::MissingNode {
                        what: "SIPCALLSLOTS",
                    })?;
                Ok(vec![change_frame::encode_property_changed(
                    &path,
                    param.as_str(),
                    value,
                )])
            }
            Command::SetTestParam { param, value } => {
                let path = layout
                    .test_path()
                    .ok_or(EncodeError::MissingNode { what: "TEST" })?;
                Ok(vec![change_frame::encode_property_changed(
                    &path,
                    param.as_str(),
                    value,
                )])
            }
            Command::SetPadRecorderParam {
                pad_recorder,
                param,
                value,
            } => {
                let path =
                    layout
                        .pad_recorder_path(*pad_recorder)
                        .ok_or(EncodeError::MissingNode {
                            what: "PADRECORDER",
                        })?;
                Ok(vec![change_frame::encode_property_changed(
                    &path,
                    param.as_str(),
                    value,
                )])
            }
            Command::SetFxPresetParam {
                preset,
                param,
                value,
            } => {
                let path = layout
                    .fx_preset_path(*preset)
                    .ok_or(EncodeError::MissingNode { what: "FXPRESET" })?;
                Ok(vec![change_frame::encode_property_changed(
                    &path,
                    param.as_str(),
                    value,
                )])
            }
            Command::SetNetworkParam { param, value } => {
                let path = layout
                    .network_path()
                    .ok_or(EncodeError::MissingNode { what: "NETWORK" })?;
                Ok(vec![change_frame::encode_property_changed(
                    &path,
                    param.as_str(),
                    value,
                )])
            }
            Command::SetAudioParam { param, value } => {
                let path = layout
                    .audio_path()
                    .ok_or(EncodeError::MissingNode { what: "AUDIO" })?;
                Ok(vec![change_frame::encode_property_changed(
                    &path,
                    param.as_str(),
                    value,
                )])
            }
            Command::SetBuildParam { param, value } => {
                let path = layout
                    .build_path()
                    .ok_or(EncodeError::MissingNode { what: "BUILD" })?;
                Ok(vec![change_frame::encode_property_changed(
                    &path,
                    param.as_str(),
                    value,
                )])
            }
            Command::SetAppParam { param, value } => {
                let path = layout
                    .app_path()
                    .ok_or(EncodeError::MissingNode { what: "APP" })?;
                Ok(vec![change_frame::encode_property_changed(
                    &path,
                    param.as_str(),
                    value,
                )])
            }
            Command::SetThemeParam { param, value } => {
                let path = layout
                    .theme_path()
                    .ok_or(EncodeError::MissingNode { what: "THEME" })?;
                Ok(vec![change_frame::encode_property_changed(
                    &path,
                    param.as_str(),
                    value,
                )])
            }
            Command::SetCurrentShowParam { param, value } => {
                let path = layout.current_show_path().ok_or(EncodeError::MissingNode {
                    what: "CURRENTSHOW",
                })?;
                Ok(vec![change_frame::encode_property_changed(
                    &path,
                    param.as_str(),
                    value,
                )])
            }
            Command::SetShowControlParam { param, value } => {
                let path = layout.show_control_path().ok_or(EncodeError::MissingNode {
                    what: "SHOWCONTROL",
                })?;
                Ok(vec![change_frame::encode_property_changed(
                    &path,
                    param.as_str(),
                    value,
                )])
            }
            Command::SetRecordingsParam { param, value } => {
                let path = layout
                    .recordings_path()
                    .ok_or(EncodeError::MissingNode { what: "RECORDINGS" })?;
                Ok(vec![change_frame::encode_property_changed(
                    &path,
                    param.as_str(),
                    value,
                )])
            }
            Command::SetRadioParam { param, value } => {
                let path = layout
                    .radio_path()
                    .ok_or(EncodeError::MissingNode { what: "RADIO" })?;
                Ok(vec![change_frame::encode_property_changed(
                    &path,
                    param.as_str(),
                    value,
                )])
            }
            Command::SetShowParam { show, param, value } => {
                let path = layout
                    .show_path(*show)
                    .ok_or(EncodeError::MissingNode { what: "SHOW" })?;
                Ok(vec![change_frame::encode_property_changed(
                    &path,
                    param.as_str(),
                    value,
                )])
            }
            Command::SetRecordingParam {
                recording,
                param,
                value,
            } => {
                let path = layout
                    .recording_path(*recording)
                    .ok_or(EncodeError::MissingNode { what: "RECORDING" })?;
                Ok(vec![change_frame::encode_property_changed(
                    &path,
                    param.as_str(),
                    value,
                )])
            }
            Command::SetStorageVolumeParam {
                volume,
                param,
                value,
            } => {
                let path = layout
                    .storage_volume_path(*volume)
                    .ok_or(EncodeError::MissingNode {
                        what: "STORAGEVOLUME",
                    })?;
                Ok(vec![change_frame::encode_property_changed(
                    &path,
                    param.as_str(),
                    value,
                )])
            }
            Command::SetRadioTxParam { tx, param, value } => {
                let path = layout
                    .radio_tx_path(*tx)
                    .ok_or(EncodeError::MissingNode { what: "RADIOTX" })?;
                Ok(vec![change_frame::encode_property_changed(
                    &path,
                    param.as_str(),
                    value,
                )])
            }
            Command::SetRadioRxParam { rx, param, value } => {
                let path = layout
                    .radio_rx_path(*rx)
                    .ok_or(EncodeError::MissingNode { what: "RADIORX" })?;
                Ok(vec![change_frame::encode_property_changed(
                    &path,
                    param.as_str(),
                    value,
                )])
            }
            Command::SetWifiScanResultParam { slot, param, value } => {
                let path = layout
                    .wifi_scan_result_path(*slot)
                    .ok_or(EncodeError::MissingNode {
                        what: "WIFISCANRESULT",
                    })?;
                Ok(vec![change_frame::encode_property_changed(
                    &path,
                    param.as_str(),
                    value,
                )])
            }
            Command::SetStreamerXMixPresetParam {
                preset,
                param,
                value,
            } => {
                let path =
                    layout
                        .streamerx_mix_preset_path(*preset)
                        .ok_or(EncodeError::MissingNode {
                            what: "STREAMERXMIXPRESET",
                        })?;
                Ok(vec![change_frame::encode_property_changed(
                    &path,
                    param.as_str(),
                    value,
                )])
            }
            Command::SetStreamerXStreamMixParam {
                stream,
                param,
                value,
            } => {
                let path =
                    layout
                        .streamerx_stream_mix_path(*stream)
                        .ok_or(EncodeError::MissingNode {
                            what: "STREAMERXSTREAMMIX",
                        })?;
                Ok(vec![change_frame::encode_property_changed(
                    &path,
                    param.as_str(),
                    value,
                )])
            }
            Command::SetRcSyncMixParam { mix, param, value } => {
                let path = layout
                    .rcsync_mix_path(*mix)
                    .ok_or(EncodeError::MissingNode { what: "RCSYNCMIX" })?;
                Ok(vec![change_frame::encode_property_changed(
                    &path,
                    param.as_str(),
                    value,
                )])
            }
            Command::SetMixMinusesParam {
                minuses,
                param,
                value,
            } => {
                let path = layout
                    .mix_minuses_path(*minuses)
                    .ok_or(EncodeError::MissingNode { what: "MIXMINUSES" })?;
                Ok(vec![change_frame::encode_property_changed(
                    &path,
                    param.as_str(),
                    value,
                )])
            }
            Command::SetupSkip { language, timezone } => {
                let mut frames = Vec::new();
                if let Some(lang) = language {
                    if let Some(path) = layout.gui_path() {
                        frames.push(change_frame::encode_property_changed(
                            &path,
                            "lang",
                            &Value::String(lang.clone()),
                        ));
                    }
                }
                if let Some(tz) = timezone {
                    if let Some(path) = layout.system_path() {
                        frames.push(change_frame::encode_property_changed(
                            &path,
                            "systemDateTimezone",
                            &Value::String(tz.clone()),
                        ));
                        frames.push(change_frame::encode_property_changed(
                            &path,
                            "systemDateTime24h",
                            &Value::Bool(true),
                        ));
                        frames.push(change_frame::encode_property_changed(
                            &path,
                            "systemDateTimeOnHome",
                            &Value::Bool(true),
                        ));
                    }
                }
                if let Some(path) = layout.show_control_path() {
                    frames.push(change_frame::encode_property_changed(
                        &path,
                        "showControlNewFromDefaultMuted",
                        &Value::Bool(true),
                    ));
                }
                if let Some(path) = layout.system_path() {
                    frames.push(change_frame::encode_property_changed(
                        &path,
                        "disableAllPhysicalButtons",
                        &Value::Bool(false),
                    ));
                    frames.push(change_frame::encode_property_changed(
                        &path,
                        "disableAllLineoutOutputs",
                        &Value::Bool(false),
                    ));
                    frames.push(change_frame::encode_property_changed(
                        &path,
                        "disableAllHeadphoneOutputs",
                        &Value::Bool(false),
                    ));
                    frames.push(change_frame::encode_property_changed(
                        &path,
                        "systemChannelSelected",
                        &Value::Int(-1),
                    ));
                }
                Ok(frames)
            }
        }
    }
}

/// Resolve a named fader strip to its wire fader/channel index on the layout's
/// device model. `Err(FaderNotOnModel)` if the strip does not exist there
/// (e.g. `Physical6` on a Duo).
fn fader_index(layout: &Layout, fader: Fader) -> Result<u8, EncodeError> {
    fader
        .to_index(layout.model())
        .ok_or(EncodeError::FaderNotOnModel {
            fader,
            model: layout.model(),
        })
}

/// Single-level request path for a CallMe routing cell. See
/// [`CALLME_MIX_PATH_OFFSET`].
fn callme_request_path(source: Source, mix: MixOutput) -> Vec<u32> {
    vec![((source.to_protocol() as u32) << 8) | (CALLME_MIX_PATH_OFFSET + mix.to_protocol() as u32)]
}

fn channel_path(layout: &Layout, idx: u8) -> Result<Vec<u32>, EncodeError> {
    layout.channel_path(idx).ok_or(EncodeError::OutOfRange {
        what: "fader",
        index: idx as u32,
        bound: layout.channel_count() as u32,
    })
}

fn input_source_path(layout: &Layout, source: Source) -> Result<Vec<u32>, EncodeError> {
    let idx = source.to_protocol();
    layout
        .input_source_path(idx)
        .ok_or(EncodeError::OutOfRange {
            what: "input source",
            index: idx as u32,
            bound: layout.input_source_count() as u32,
        })
}

fn fader_path(layout: &Layout, idx: u8) -> Result<Vec<u32>, EncodeError> {
    layout.fader_path(idx).ok_or(EncodeError::OutOfRange {
        what: "fader",
        index: idx as u32,
        bound: layout.fader_count() as u32,
    })
}

fn headphone_path(layout: &Layout, idx: u8) -> Result<Vec<u32>, EncodeError> {
    layout.headphone_path(idx).ok_or(EncodeError::OutOfRange {
        what: "headphone",
        index: idx as u32,
        bound: layout.headphone_count() as u32,
    })
}

fn effects_path(layout: &Layout, idx: u8) -> Result<Vec<u32>, EncodeError> {
    layout.effects_path(idx).ok_or(EncodeError::OutOfRange {
        what: "effects",
        index: idx as u32,
        bound: layout.effects_count() as u32,
    })
}

fn pad_path(layout: &Layout, idx: u8) -> Result<Vec<u32>, EncodeError> {
    layout.pad_path(idx).ok_or(EncodeError::OutOfRange {
        what: "pad",
        index: idx as u32,
        bound: layout.pad_count() as u32,
    })
}

fn mix_path(layout: &Layout, source: Source, mix: MixOutput) -> Result<Vec<u32>, EncodeError> {
    layout
        .mix_cell_path(source.to_protocol(), mix.to_protocol())
        .ok_or(EncodeError::MixCellOutOfRange {
            source: source.to_protocol(),
            mix: mix.to_protocol(),
            source_bound: layout.source_count(),
            mix_bound: layout.mix_count_per_source(),
        })
}

#[derive(Debug, Clone, PartialEq)]
pub enum EncodeError {
    OutOfRange {
        what: &'static str,
        index: u32,
        bound: u32,
    },
    MixCellOutOfRange {
        source: u8,
        mix: u8,
        source_bound: u8,
        mix_bound: u8,
    },
    /// The named fader strip does not exist on this device model (e.g.
    /// `Physical6` on a Duo, or `Virtual4` on a Pro II).
    FaderNotOnModel { fader: Fader, model: DeviceModel },
    /// A singleton node the command addresses (e.g. `MASTERCHANNEL`, `OUTPUT`)
    /// was not present in the layout's fullSync. Unlike [`EncodeError::OutOfRange`]
    /// there is no index: the whole family is absent.
    MissingNode { what: &'static str },
}

impl std::fmt::Display for EncodeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EncodeError::OutOfRange { what, index, bound } => {
                write!(f, "{what} index {index} out of range (bound {bound})")
            }
            EncodeError::FaderNotOnModel { fader, model } => {
                write!(f, "fader {fader} does not exist on {model}")
            }
            EncodeError::MissingNode { what } => {
                write!(f, "layout has no {what} node")
            }
            EncodeError::MixCellOutOfRange {
                source,
                mix,
                source_bound,
                mix_bound,
            } => write!(
                f,
                "mix cell ({source}, {mix}) out of range (sources {source_bound}, mixes {mix_bound})"
            ),
        }
    }
}

impl std::error::Error for EncodeError {}

#[cfg(test)]
#[path = "commands_tests.rs"]
mod tests;
