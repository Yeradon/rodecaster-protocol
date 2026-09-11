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

/// Canonical trigger payload alias for backward compatibility in tests.
#[cfg(test)]
const MIX_LINK_TRIGGER: [u8; 6] = crate::trigger::REQUEST_BYTES;

/// JUCE `Int` value representing unassigned for `channelInputSource`.
const CHANNEL_INPUT_SOURCE_UNASSIGNED: i64 = -1;

/// Verbatim wire bytes for the screen-wake message ([`Command::ScreenTouched`]).
///
/// Anatomy: PropertyChanged changeType (0x01), path depth 1 (0x01, 0x01),
/// child node 1 prefix (0x01), followed by null-terminated "screenTouched\0".
/// Emitted verbatim by RØDE Central to wake the touchscreen display on node index 1.
const SCREEN_TOUCHED_FRAME: [u8; 18] = [
    0x01, 0x01, 0x01, 0x01, b's', b'c', b'r', b'e', b'e', b'n', b'T', b'o', b'u', b'c', b'h', b'e',
    b'd', 0x00,
];

/// Mix offset index for CallMe return channel routing requests.
///
/// Used in `(source << 8) | (CALLME_MIX_PATH_OFFSET + mix)` to address
/// external SIP/CallMe return channels outside the primary mix matrix.
const CALLME_MIX_PATH_OFFSET: u32 = 4;

#[inline]
fn single_prop_frame(path: &[u32], name: &str, value: &Value) -> Vec<Vec<u8>> {
    vec![change_frame::encode_property_changed(path, name, value)]
}

#[inline]
fn trigger_frame(path: &[u32], name: &str) -> Vec<Vec<u8>> {
    single_prop_frame(path, name, &crate::trigger::request_value())
}

macro_rules! encode_singleton {
    ($layout:expr, $getter:ident, $what:literal, $param:expr, $val:expr) => {{
        let path = $layout
            .$getter()
            .ok_or(EncodeError::MissingNode { what: $what })?;
        Ok(single_prop_frame(&path, $param.as_str(), $val))
    }};
}

macro_rules! encode_indexed {
    ($layout:expr, $getter:ident, $idx:expr, $what:literal, $param:expr, $val:expr) => {{
        let path = $layout
            .$getter($idx)
            .ok_or(EncodeError::MissingNode { what: $what })?;
        Ok(single_prop_frame(&path, $param.as_str(), $val))
    }};
}

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
    /// Set a channel-strip DSP parameter on a fader's CHANNEL node.
    SetChannelParam {
        fader: Fader,
        param: ChannelParam,
        value: Value,
    },
    /// Set an input-source parameter on a source's INPUTSOURCE node.
    SetInputSourceParam {
        source: Source,
        param: InputSourceParam,
        value: Value,
    },
    /// Set a master-bus parameter on the MASTERCHANNEL node.
    SetMasterParam { param: MasterParam, value: Value },
    /// Set an output-bus parameter on the OUTPUT node.
    SetOutputParam { param: OutputParam, value: Value },
    /// Set the auto-duck depth on the DUCKER node.
    SetDuckerParam { param: DuckerParam, value: Value },
    /// Set a recorder transport property on the RECORDER node.
    SetRecorderParam { param: RecorderParam, value: Value },
    /// Set a player property on the PLAYER node.
    SetPlayerParam { param: PlayerParam, value: Value },
    /// Set a headphone property on a HEADPHONE node.
    SetHeadphoneParam {
        headphone: u8,
        param: HeadphoneParam,
        value: Value,
    },
    /// Set an effects parameter on an EFFECTS_PARAMETERS node.
    SetEffectsParam {
        effects: u8,
        param: EffectsParam,
        value: Value,
    },
    /// Set a front-panel UI parameter on the GUI node.
    SetGuiParam { param: GuiParam, value: Value },
    /// Set a sound-pad parameter on a PAD node.
    SetPadParam {
        pad: u8,
        param: PadParam,
        value: Value,
    },
    /// Set a device-wide system parameter on the SYSTEM node.
    SetSystemParam { param: SystemParam, value: Value },
    /// Set a SIP calling parameter on the SIPCALLING node.
    SetSipCallingParam {
        param: SipCallingParam,
        value: Value,
    },
    /// Set a per-registration SIP parameter on a SIPREGISTRATION node.
    SetSipRegistrationParam {
        registration: u8,
        param: SipRegistrationParam,
        value: Value,
    },
    /// Set a per-call-slot SIP parameter on a SIPCALLSLOTS node.
    SetSipCallSlotsParam {
        slot: u8,
        param: SipCallSlotsParam,
        value: Value,
    },
    /// Set a SIP advanced parameter on the SIPADVANCED node.
    SetSipAdvancedParam {
        param: SipAdvancedParam,
        value: Value,
    },
    /// Set a diagnostic parameter on the TEST node.
    SetTestParam { param: TestParam, value: Value },
    /// Set a pad recorder parameter on a PADRECORDER node.
    SetPadRecorderParam {
        pad_recorder: u8,
        param: PadRecorderParam,
        value: Value,
    },
    /// Set an effects preset parameter on an FXPRESET node.
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
                        &crate::trigger::request_value(),
                    ),
                ])
            }
            Command::UnlinkMix { source, mix } => {
                let path = mix_path(layout, *source, *mix)?;
                Ok(trigger_frame(&path, "mixUnlinkRequest"))
            }
            Command::ScreenTouched => Ok(vec![SCREEN_TOUCHED_FRAME.to_vec()]),
            Command::PowerOff => {
                let path = layout
                    .system_path()
                    .ok_or(EncodeError::MissingNode { what: "SYSTEM" })?;
                Ok(single_prop_frame(
                    &path,
                    "powerOffRequest",
                    &Value::Bool(true),
                ))
            }
            Command::LinkCallMe { source, mix } => Ok(trigger_frame(
                &callme_request_path(*source, *mix),
                "mixLinkRequest",
            )),
            Command::UnlinkCallMe { source, mix } => Ok(trigger_frame(
                &callme_request_path(*source, *mix),
                "mixUnlinkRequest",
            )),
            Command::SetChannelParam {
                fader,
                param,
                value,
            } => {
                let path = channel_path(layout, fader_index(layout, *fader)?)?;
                Ok(single_prop_frame(&path, param.as_str(), value))
            }
            Command::SetInputSourceParam {
                source,
                param,
                value,
            } => {
                let path = input_source_path(layout, *source)?;
                Ok(single_prop_frame(&path, param.as_str(), value))
            }
            Command::SetMasterParam { param, value } => {
                encode_singleton!(layout, master_channel_path, "MASTERCHANNEL", param, value)
            }
            Command::SetOutputParam { param, value } => {
                encode_singleton!(layout, output_path, "OUTPUT", param, value)
            }
            Command::SetDuckerParam { param, value } => {
                encode_singleton!(layout, ducker_path, "DUCKER", param, value)
            }
            Command::SetRecorderParam { param, value } => {
                encode_singleton!(layout, recorder_path, "RECORDER", param, value)
            }
            Command::SetPlayerParam { param, value } => {
                encode_singleton!(layout, player_path, "PLAYER", param, value)
            }
            Command::SetHeadphoneParam {
                headphone,
                param,
                value,
            } => {
                let path = headphone_path(layout, *headphone)?;
                Ok(single_prop_frame(&path, param.as_str(), value))
            }
            Command::SetEffectsParam {
                effects,
                param,
                value,
            } => {
                let path = effects_path(layout, *effects)?;
                Ok(single_prop_frame(&path, param.as_str(), value))
            }
            Command::SetGuiParam { param, value } => {
                encode_singleton!(layout, gui_path, "GUI", param, value)
            }
            Command::SetPadParam { pad, param, value } => {
                let path = pad_path(layout, *pad)?;
                Ok(single_prop_frame(&path, param.as_str(), value))
            }
            Command::SetSystemParam { param, value } => {
                encode_singleton!(layout, system_path, "SYSTEM", param, value)
            }
            Command::SetSipCallingParam { param, value } => {
                encode_singleton!(layout, sip_calling_path, "SIPCALLING", param, value)
            }
            Command::SetSipAdvancedParam { param, value } => {
                encode_singleton!(layout, sip_advanced_path, "SIPADVANCED", param, value)
            }
            Command::SetSipRegistrationParam {
                registration,
                param,
                value,
            } => {
                encode_indexed!(
                    layout,
                    sip_registration_path,
                    *registration,
                    "SIPREGISTRATION",
                    param,
                    value
                )
            }
            Command::SetSipCallSlotsParam { slot, param, value } => {
                encode_indexed!(
                    layout,
                    sip_call_slots_path,
                    *slot,
                    "SIPCALLSLOTS",
                    param,
                    value
                )
            }
            Command::SetTestParam { param, value } => {
                encode_singleton!(layout, test_path, "TEST", param, value)
            }
            Command::SetPadRecorderParam {
                pad_recorder,
                param,
                value,
            } => {
                encode_indexed!(
                    layout,
                    pad_recorder_path,
                    *pad_recorder,
                    "PADRECORDER",
                    param,
                    value
                )
            }
            Command::SetFxPresetParam {
                preset,
                param,
                value,
            } => {
                encode_indexed!(layout, fx_preset_path, *preset, "FXPRESET", param, value)
            }
            Command::SetNetworkParam { param, value } => {
                encode_singleton!(layout, network_path, "NETWORK", param, value)
            }
            Command::SetAudioParam { param, value } => {
                encode_singleton!(layout, audio_path, "AUDIO", param, value)
            }
            Command::SetBuildParam { param, value } => {
                encode_singleton!(layout, build_path, "BUILD", param, value)
            }
            Command::SetAppParam { param, value } => {
                encode_singleton!(layout, app_path, "APP", param, value)
            }
            Command::SetThemeParam { param, value } => {
                encode_singleton!(layout, theme_path, "THEME", param, value)
            }
            Command::SetCurrentShowParam { param, value } => {
                encode_singleton!(layout, current_show_path, "CURRENTSHOW", param, value)
            }
            Command::SetShowControlParam { param, value } => {
                encode_singleton!(layout, show_control_path, "SHOWCONTROL", param, value)
            }
            Command::SetRecordingsParam { param, value } => {
                encode_singleton!(layout, recordings_path, "RECORDINGS", param, value)
            }
            Command::SetRadioParam { param, value } => {
                encode_singleton!(layout, radio_path, "RADIO", param, value)
            }
            Command::SetShowParam { show, param, value } => {
                encode_indexed!(layout, show_path, *show, "SHOW", param, value)
            }
            Command::SetRecordingParam {
                recording,
                param,
                value,
            } => {
                encode_indexed!(
                    layout,
                    recording_path,
                    *recording,
                    "RECORDING",
                    param,
                    value
                )
            }
            Command::SetStorageVolumeParam {
                volume,
                param,
                value,
            } => {
                encode_indexed!(
                    layout,
                    storage_volume_path,
                    *volume,
                    "STORAGEVOLUME",
                    param,
                    value
                )
            }
            Command::SetRadioTxParam { tx, param, value } => {
                encode_indexed!(layout, radio_tx_path, *tx, "RADIOTX", param, value)
            }
            Command::SetRadioRxParam { rx, param, value } => {
                encode_indexed!(layout, radio_rx_path, *rx, "RADIORX", param, value)
            }
            Command::SetWifiScanResultParam { slot, param, value } => {
                encode_indexed!(
                    layout,
                    wifi_scan_result_path,
                    *slot,
                    "WIFISCANRESULT",
                    param,
                    value
                )
            }
            Command::SetStreamerXMixPresetParam {
                preset,
                param,
                value,
            } => {
                encode_indexed!(
                    layout,
                    streamerx_mix_preset_path,
                    *preset,
                    "STREAMERXMIXPRESET",
                    param,
                    value
                )
            }
            Command::SetStreamerXStreamMixParam {
                stream,
                param,
                value,
            } => {
                encode_indexed!(
                    layout,
                    streamerx_stream_mix_path,
                    *stream,
                    "STREAMERXSTREAMMIX",
                    param,
                    value
                )
            }
            Command::SetRcSyncMixParam { mix, param, value } => {
                encode_indexed!(layout, rcsync_mix_path, *mix, "RCSYNCMIX", param, value)
            }
            Command::SetMixMinusesParam {
                minuses,
                param,
                value,
            } => {
                encode_indexed!(
                    layout,
                    mix_minuses_path,
                    *minuses,
                    "MIXMINUSES",
                    param,
                    value
                )
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
