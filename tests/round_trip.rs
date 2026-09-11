//! End-to-end round-trip integration tests.
//!
//! Build a synthetic fullSync tree, derive a [`Layout`] from it, then verify
//! every typed [`Command`] survives the full pipeline:
//!
//! ```text
//!   Command::encode(&Layout)
//!     -> change-frame payload bytes
//!     -> change_frame::decode
//!     -> decode_event(payload, &Layout)
//!     -> DeviceEvent with the original logical address
//! ```
//!
//! These exercise the *integration* between the layers; per-module unit tests
//! cover each layer in isolation.

use rodecaster_protocol::test_fixtures::{n, nc, np, prop};
use rodecaster_protocol::{
    change_frame::{decode as decode_change_frame, encode_property_changed, ChangeFrame},
    decode_event, ChannelParam, Command, DeviceEvent, DuckerParam, EffectsParam, Fader, GuiParam,
    HeadphoneParam, InputSourceParam, Layout, MasterParam, MixLinkDirection, MixOutput, Node,
    OutputParam, PadParam, PlayerParam, RecorderParam, Source, SystemParam, TriggerPhase, Value,
};

/// Synthetic fullSync that puts the addressable families at NON-default
/// positions, so every test below fails if any code accidentally hardcodes
/// 0x1C, 0x04, or 62.
fn synthetic_root() -> Node {
    let phys = nc(
        "PHYSICALINTERFACE",
        vec![
            n("HEADER"),
            np("FADER", vec![prop("faderLevel", Value::Int(50))]),
            np("FADER", vec![prop("faderLevel", Value::Int(60))]),
            np("FADER", vec![prop("faderLevel", Value::Int(70))]),
            n("FOOTER"),
        ],
    );

    let mut children = vec![
        n("OTHER1"),
        n("OTHER2"),
        phys, // physical_interface_idx = 2 (NOT 0)
        n("OTHER3"),
        np(
            "CHANNEL",
            vec![
                prop("channelOutputMute", Value::Bool(false)),
                prop("channelCueEnable", Value::Bool(false)),
            ],
        ),
        np(
            "CHANNEL",
            vec![prop("channelOutputMute", Value::Bool(true))],
        ),
        np(
            "CHANNEL",
            vec![prop("channelOutputMute", Value::Bool(false))],
        ),
        n("OTHER4"),
    ];
    // 26 MIX nodes => 2 sources × 13 mixes.
    for i in 0..26 {
        children.push(np(
            "MIX",
            vec![prop(
                "mixLevelWithAnchor",
                Value::String(format!("0.5|{}", 0.1 * (i % 13) as f32)),
            )],
        ));
    }
    // 19 INPUTSOURCE nodes (addressable source run), property-less so they don't
    // change the initial-state event count assertions below.
    for _ in 0..19 {
        children.push(n("INPUTSOURCE"));
    }
    // Singleton families, property-less for the same reason (no extra events).
    children.push(n("MASTERCHANNEL"));
    children.push(n("OUTPUT"));
    children.push(n("DUCKER"));
    children.push(n("RECORDER"));
    children.push(n("PLAYER"));
    // HEADPHONE is multi-instance; two at the tail (run length 2), property-less.
    children.push(n("HEADPHONE"));
    children.push(n("HEADPHONE"));
    // EFFECTS_PARAMETERS is multi-instance; three at the tail (run length 3),
    // property-less so the initial-state event count assertions are unaffected.
    children.push(n("EFFECTS_PARAMETERS"));
    children.push(n("EFFECTS_PARAMETERS"));
    children.push(n("EFFECTS_PARAMETERS"));
    // GUI is the single front-panel UI-state node (singleton), property-less so
    // the initial-state event count assertions are unaffected.
    children.push(n("GUI"));
    // SOUNDPADS container with property-less PAD nodes, so the initial-state
    // event count assertions are unaffected (pads carry no props here).
    children.push(nc("SOUNDPADS", vec![n("PAD"), n("PAD")]));
    // SYSTEM is the device-level singleton (identity, clock, update lifecycle),
    // property-less here so the initial-state event count assertions are unaffected.
    children.push(n("SYSTEM"));
    nc("DEVICE", children)
}

fn layout() -> Layout {
    Layout::from_full_sync(&synthetic_root()).unwrap()
}

#[test]
fn round_trip_set_fader_mute() {
    let l = layout();
    let cmd = Command::SetFaderMute {
        fader: Fader::Physical2,
        mute: true,
    };
    let payloads = cmd.encode(&l).unwrap();
    assert_eq!(payloads.len(), 1);

    let event = decode_event(&payloads[0], &l).unwrap();
    assert_eq!(
        event,
        DeviceEvent::FaderMuteChanged {
            fader: Fader::Physical2,
            muted: true,
        }
    );
}

#[test]
fn round_trip_set_fader_cue() {
    let l = layout();
    let cmd = Command::SetFaderCue {
        fader: Fader::Physical1,
        enable: true,
    };
    let payloads = cmd.encode(&l).unwrap();
    let event = decode_event(&payloads[0], &l).unwrap();
    assert_eq!(
        event,
        DeviceEvent::FaderCueChanged {
            fader: Fader::Physical1,
            enabled: true,
        }
    );
}

#[test]
fn round_trip_set_fader_level_through_two_level_path() {
    let l = layout();
    let cmd = Command::SetFaderLevel {
        fader: Fader::Physical3,
        level: 100,
    };
    let payloads = cmd.encode(&l).unwrap();
    let event = decode_event(&payloads[0], &l).unwrap();
    assert_eq!(
        event,
        DeviceEvent::FaderLevelChanged {
            fader: Fader::Physical3,
            level: 100,
        }
    );
}

#[test]
fn round_trip_set_mix_disabled() {
    let l = layout();
    let cmd = Command::SetMixDisabled {
        source: Source::Combo2,
        mix: MixOutput::Usb1,
        disabled: true,
    };
    let payloads = cmd.encode(&l).unwrap();
    let event = decode_event(&payloads[0], &l).unwrap();
    assert_eq!(
        event,
        DeviceEvent::MixDisabledChanged {
            source: Source::Combo2,
            mix: MixOutput::Usb1,
            disabled: true,
        }
    );
}

#[test]
fn round_trip_set_mix_level() {
    let l = layout();
    let cmd = Command::SetMixLevel {
        source: Source::Combo2,
        mix: MixOutput::Usb1,
        anchor: 0.5,
        value: 0.8,
    };
    let payloads = cmd.encode(&l).unwrap();
    let event = decode_event(&payloads[0], &l).unwrap();
    match event {
        DeviceEvent::MixLevelChanged {
            source,
            mix,
            anchor,
            value,
        } => {
            assert_eq!(source, Source::Combo2);
            assert_eq!(mix, MixOutput::Usb1);
            assert!((anchor - 0.5).abs() < 1e-4);
            assert!((value - 0.8).abs() < 1e-4);
        }
        other => panic!("expected MixLevelChanged, got {other:?}"),
    }
}

#[test]
fn round_trip_set_channel_param() {
    let l = layout();
    // Physical2 -> channel index 1. Unlike channelInputSource (which the device
    // echoes at stride 6), the DSP strip params are stride-1 both ways, so the
    // logical address survives the round-trip unchanged.
    let cmd = Command::SetChannelParam {
        fader: Fader::Physical2,
        param: ChannelParam::EqHighGain,
        value: Value::Double(6.0),
    };
    let payloads = cmd.encode(&l).unwrap();
    assert_eq!(payloads.len(), 1);

    let event = decode_event(&payloads[0], &l).unwrap();
    assert_eq!(
        event,
        DeviceEvent::ChannelParamChanged {
            fader: Fader::Physical2,
            param: ChannelParam::EqHighGain,
            value: Value::Double(6.0),
        }
    );
}

#[test]
fn round_trip_set_channel_param_preserves_every_value_shape() {
    let l = layout();
    // The JUCE wire is self-describing, so whatever Value type the caller picks
    // survives byte-faithfully through encode -> decode. Covers each marker and
    // the Other (untyped name) forward-compat path.
    let cases = [
        (ChannelParam::EqOn, Value::Bool(true)),
        (ChannelParam::CompressorRatio, Value::Int(4)),
        (ChannelParam::HpfFrequency, Value::Double(80.0)),
        (
            ChannelParam::Other("channelCustomLabel".to_string()),
            Value::String("xlr".to_string()),
        ),
        (
            ChannelParam::Other("channelMysteryKnob".to_string()),
            Value::Int(3),
        ),
    ];
    for (param, value) in cases {
        let cmd = Command::SetChannelParam {
            fader: Fader::Physical1,
            param: param.clone(),
            value: value.clone(),
        };
        let payloads = cmd.encode(&l).unwrap();
        assert_eq!(
            decode_event(&payloads[0], &l).unwrap(),
            DeviceEvent::ChannelParamChanged {
                fader: Fader::Physical1,
                param,
                value,
            }
        );
    }
}

#[test]
fn round_trip_set_input_source_param() {
    let l = layout();
    // Combo2 -> source ordinal 1 -> 2nd INPUTSOURCE node. INPUTSOURCE params are
    // stride-1 both ways (no echo asymmetry), so the source survives unchanged.
    let cmd = Command::SetInputSourceParam {
        source: Source::Combo2,
        param: InputSourceParam::InputMicrophoneGain,
        value: Value::Int(50),
    };
    let payloads = cmd.encode(&l).unwrap();
    assert_eq!(payloads.len(), 1);

    let event = decode_event(&payloads[0], &l).unwrap();
    assert_eq!(
        event,
        DeviceEvent::InputSourceParamChanged {
            source: Source::Combo2,
            param: InputSourceParam::InputMicrophoneGain,
            value: Value::Int(50),
        }
    );
}

#[test]
fn round_trip_set_input_source_param_preserves_every_value_shape() {
    let l = layout();
    // Same self-describing-wire guarantee as the channel-param shape test, but on
    // the INPUTSOURCE node. InputWirelessSn is genuinely a String on the device.
    let cases = [
        (InputSourceParam::InputPower, Value::Int(1)),
        (InputSourceParam::InputPhaseFlip, Value::Bool(true)),
        (
            InputSourceParam::InputWirelessSn,
            Value::String("SN12345".to_string()),
        ),
        (
            InputSourceParam::Other("inputMysteryFlag".to_string()),
            Value::Int(7),
        ),
    ];
    for (param, value) in cases {
        let cmd = Command::SetInputSourceParam {
            source: Source::Combo1,
            param: param.clone(),
            value: value.clone(),
        };
        let payloads = cmd.encode(&l).unwrap();
        assert_eq!(
            decode_event(&payloads[0], &l).unwrap(),
            DeviceEvent::InputSourceParamChanged {
                source: Source::Combo1,
                param,
                value,
            }
        );
    }
}

#[test]
fn round_trip_set_master_param() {
    let l = layout();
    // Key-less singleton: the master Compellor threshold, a Double, must land on
    // the MASTERCHANNEL node and decode back to the same key-less event.
    let cmd = Command::SetMasterParam {
        param: MasterParam::CompellorThreshold,
        value: Value::Double(0.67),
    };
    let payloads = cmd.encode(&l).unwrap();
    assert_eq!(payloads.len(), 1);

    let event = decode_event(&payloads[0], &l).unwrap();
    assert_eq!(
        event,
        DeviceEvent::MasterParamChanged {
            param: MasterParam::CompellorThreshold,
            value: Value::Double(0.67),
        }
    );
}

#[test]
fn round_trip_set_output_param() {
    let l = layout();
    let cmd = Command::SetOutputParam {
        param: OutputParam::MonLevel,
        value: Value::Double(0.0),
    };
    let payloads = cmd.encode(&l).unwrap();
    assert_eq!(payloads.len(), 1);

    let event = decode_event(&payloads[0], &l).unwrap();
    assert_eq!(
        event,
        DeviceEvent::OutputParamChanged {
            param: OutputParam::MonLevel,
            value: Value::Double(0.0),
        }
    );
}

#[test]
fn round_trip_set_master_and_output_params_preserve_every_value_shape() {
    let l = layout();
    // Same self-describing-wire guarantee on the two singleton nodes, including
    // each marker type and the Other (untyped name) forward-compat path.
    let master_cases = [
        (MasterParam::CompellorOn, Value::Bool(true)),
        (MasterParam::DelaySeconds, Value::Double(0.25)),
        (
            MasterParam::Other("masterMysteryKnob".to_string()),
            Value::Int(3),
        ),
    ];
    for (param, value) in master_cases {
        let payloads = Command::SetMasterParam {
            param: param.clone(),
            value: value.clone(),
        }
        .encode(&l)
        .unwrap();
        assert_eq!(
            decode_event(&payloads[0], &l).unwrap(),
            DeviceEvent::MasterParamChanged { param, value }
        );
    }

    let output_cases = [
        (OutputParam::MonMute, Value::Bool(false)),
        (OutputParam::MultiMode, Value::Int(5)),
        (OutputParam::BtLevel, Value::Double(0.5)),
        (
            OutputParam::Other("outputMysteryFlag".to_string()),
            Value::Int(9),
        ),
    ];
    for (param, value) in output_cases {
        let payloads = Command::SetOutputParam {
            param: param.clone(),
            value: value.clone(),
        }
        .encode(&l)
        .unwrap();
        assert_eq!(
            decode_event(&payloads[0], &l).unwrap(),
            DeviceEvent::OutputParamChanged { param, value }
        );
    }
}

#[test]
fn round_trip_set_ducker_recorder_player_params() {
    let l = layout();

    let payloads = Command::SetDuckerParam {
        param: DuckerParam::Depth,
        value: Value::Double(-9.0),
    }
    .encode(&l)
    .unwrap();
    assert_eq!(payloads.len(), 1);
    assert_eq!(
        decode_event(&payloads[0], &l).unwrap(),
        DeviceEvent::DuckerParamChanged {
            param: DuckerParam::Depth,
            value: Value::Double(-9.0),
        }
    );

    // The recorder command channel: requestRecordState round-trips as an event.
    let payloads = Command::SetRecorderParam {
        param: RecorderParam::RequestRecordState,
        value: Value::Int(1),
    }
    .encode(&l)
    .unwrap();
    assert_eq!(
        decode_event(&payloads[0], &l).unwrap(),
        DeviceEvent::RecorderParamChanged {
            param: RecorderParam::RequestRecordState,
            value: Value::Int(1),
        }
    );

    let payloads = Command::SetPlayerParam {
        param: PlayerParam::State,
        value: Value::Int(0),
    }
    .encode(&l)
    .unwrap();
    assert_eq!(
        decode_event(&payloads[0], &l).unwrap(),
        DeviceEvent::PlayerParamChanged {
            param: PlayerParam::State,
            value: Value::Int(0),
        }
    );
}

#[test]
fn round_trip_set_headphone_param_preserves_index_and_value_shape() {
    let l = layout();
    // Each headphone index + each value marker, including the Other path, must
    // survive encode -> decode addressed to the same jack.
    let cases = [
        (0u8, HeadphoneParam::Type, Value::Int(0)),
        (
            1u8,
            HeadphoneParam::Colour,
            Value::String("ffd43580".into()),
        ),
        (
            1u8,
            HeadphoneParam::Other("headphoneMystery".to_string()),
            Value::Bool(true),
        ),
    ];
    for (headphone, param, value) in cases {
        let payloads = Command::SetHeadphoneParam {
            headphone,
            param: param.clone(),
            value: value.clone(),
        }
        .encode(&l)
        .unwrap();
        assert_eq!(payloads.len(), 1);
        assert_eq!(
            decode_event(&payloads[0], &l).unwrap(),
            DeviceEvent::HeadphoneParamChanged {
                headphone,
                param,
                value,
            }
        );
    }
}

#[test]
fn round_trip_set_effects_param_preserves_index_and_value_shape() {
    let l = layout();
    // Each effects slot index + each value marker, including the Other path, must
    // survive encode -> decode addressed to the same slot. EFFECTS_PARAMETERS is
    // stride-1 both ways (no echo asymmetry), so the slot index is preserved.
    let cases = [
        (0u8, EffectsParam::ReverbOn, Value::Bool(true)),
        (1u8, EffectsParam::EchoMix, Value::Double(0.25)),
        (2u8, EffectsParam::PitchShiftSemitones, Value::Int(-3)),
        (
            2u8,
            EffectsParam::Other("flangerDepth".to_string()),
            Value::Double(0.8),
        ),
    ];
    for (effects, param, value) in cases {
        let payloads = Command::SetEffectsParam {
            effects,
            param: param.clone(),
            value: value.clone(),
        }
        .encode(&l)
        .unwrap();
        assert_eq!(payloads.len(), 1);
        assert_eq!(
            decode_event(&payloads[0], &l).unwrap(),
            DeviceEvent::EffectsParamChanged {
                effects,
                param,
                value,
            }
        );
    }
}

#[test]
fn round_trip_set_gui_param_preserves_value_shape() {
    let l = layout();
    // GUI is a key-less singleton: each value marker, including the Other path,
    // must survive encode -> decode on the single GUI node.
    let cases = [
        (GuiParam::Lang, Value::String("en".to_string())),
        (GuiParam::ScreenBrightness, Value::Int(250)),
        (GuiParam::BroadcastMeters, Value::Bool(false)),
        (GuiParam::Other("mysteryGuiKnob".to_string()), Value::Int(7)),
    ];
    for (param, value) in cases {
        let payloads = Command::SetGuiParam {
            param: param.clone(),
            value: value.clone(),
        }
        .encode(&l)
        .unwrap();
        assert_eq!(payloads.len(), 1);
        assert_eq!(
            decode_event(&payloads[0], &l).unwrap(),
            DeviceEvent::GuiParamChanged { param, value }
        );
    }
}

#[test]
fn round_trip_set_pad_param_preserves_index_and_value_shape() {
    let l = layout();
    // PAD is two-level indexed: the pad index and each value marker (including
    // the Other path) must survive encode -> decode on the addressed PAD node.
    let cases = [
        (0u8, PadParam::Name, Value::String("Applause".to_string())),
        (1, PadParam::ColourIndex, Value::Int(7)),
        (1, PadParam::Gain, Value::Double(0.5)),
        (0, PadParam::Loop, Value::Bool(true)),
        (
            1,
            PadParam::Other("padMysteryKnob".to_string()),
            Value::Int(3),
        ),
    ];
    for (pad, param, value) in cases {
        let payloads = Command::SetPadParam {
            pad,
            param: param.clone(),
            value: value.clone(),
        }
        .encode(&l)
        .unwrap();
        assert_eq!(payloads.len(), 1);
        assert_eq!(
            decode_event(&payloads[0], &l).unwrap(),
            DeviceEvent::PadParamChanged { pad, param, value }
        );
    }
}

#[test]
fn round_trip_set_system_param_preserves_value_shape() {
    let l = layout();
    // SYSTEM is a key-less singleton: each value marker, including the Other path,
    // must survive encode -> decode on the single SYSTEM node. Spot-checks the
    // casing quirk wire names (updateViaUSB) alongside the typed variants.
    let cases = [
        (
            SystemParam::FirmwareVersion,
            Value::String("1.7.3".to_string()),
        ),
        (SystemParam::BoardType, Value::Int(0)),
        (SystemParam::PowerOffRequest, Value::Bool(true)),
        (SystemParam::UpdateDownloadProgress, Value::Double(0.5)),
        (SystemParam::UpdateViaUsb, Value::Bool(true)),
        (
            SystemParam::Other("systemMysteryKnob".to_string()),
            Value::Int(7),
        ),
    ];
    for (param, value) in cases {
        let payloads = Command::SetSystemParam {
            param: param.clone(),
            value: value.clone(),
        }
        .encode(&l)
        .unwrap();
        assert_eq!(payloads.len(), 1);
        assert_eq!(
            decode_event(&payloads[0], &l).unwrap(),
            DeviceEvent::SystemParamChanged { param, value }
        );
    }
}

#[test]
fn round_trip_link_mix_emits_device_exact_sequence() {
    let l = layout();
    let cmd = Command::LinkMix {
        source: Source::Combo1,
        mix: MixOutput::Speaker,
    };
    let payloads = cmd.encode(&l).unwrap();
    assert_eq!(
        payloads.len(),
        3,
        "enable + unmute + mixLinkRequest trigger (the device's ack is its own write, \
         not something the client emits)"
    );

    // [0] enable (mixDisabled = false) -> MixDisabledChanged.
    assert_eq!(
        decode_event(&payloads[0], &l).unwrap(),
        DeviceEvent::MixDisabledChanged {
            source: Source::Combo1,
            mix: MixOutput::Speaker,
            disabled: false,
        }
    );

    // [1] unmute (mixMute = false) -> MixMuteChanged.
    assert_eq!(
        decode_event(&payloads[1], &l).unwrap(),
        DeviceEvent::MixMuteChanged {
            source: Source::Combo1,
            mix: MixOutput::Speaker,
            muted: false,
        }
    );

    // [2] mixLinkRequest = Binary trigger -> MixLinkRequested with origin =
    // ClientTrigger (byte[2] = 0x02).
    assert_eq!(
        decode_event(&payloads[2], &l).unwrap(),
        DeviceEvent::MixLinkRequested {
            source: Source::Combo1,
            mix: MixOutput::Speaker,
            direction: MixLinkDirection::Link,
            origin: TriggerPhase::Press,
        }
    );
}

#[test]
fn round_trip_unlink_mix_is_single_trigger_frame() {
    let l = layout();
    let cmd = Command::UnlinkMix {
        source: Source::Combo1,
        mix: MixOutput::Speaker,
    };
    let payloads = cmd.encode(&l).unwrap();
    assert_eq!(
        payloads.len(),
        1,
        "mixUnlinkRequest trigger; the cell's prior mixDisabled / mixMute are retained"
    );
    assert_eq!(
        decode_event(&payloads[0], &l).unwrap(),
        DeviceEvent::MixLinkRequested {
            source: Source::Combo1,
            mix: MixOutput::Speaker,
            direction: MixLinkDirection::Unlink,
            origin: TriggerPhase::Press,
        }
    );
}

#[test]
fn channel_input_source_is_asymmetric_write_stride1_echo_stride6() {
    let l = layout();
    // channelInputSource is asymmetric on firmware 1.7.3: AssignFaderSource
    // WRITES at stride 1 (channel_path), but the device ECHOES at stride 6 from
    // first_channel. Encode stays honest (one write frame); decode resolves the
    // echo addressing to the originating fader.
    let write = Command::AssignFaderSource {
        fader: Fader::Physical1,
        source: Some(Source::Combo2_3),
    }
    .encode(&l)
    .unwrap();
    assert_eq!(write.len(), 1);

    let echo = encode_property_changed(
        &[l.first_channel() + 6 * 2],
        "channelInputSource",
        &Value::Int(5),
    );
    let event = decode_event(&echo, &l).unwrap();
    assert_eq!(
        event,
        DeviceEvent::FaderAssignmentChanged {
            fader: Fader::Physical3,
            source: Some(Source::Combo2_3),
        }
    );
}

#[test]
fn full_sync_path_yields_initial_state_for_synthetic_tree() {
    let root = synthetic_root();
    let l = Layout::from_full_sync(&root).unwrap();

    // Build a synthetic FullSync change-frame: [0x02] + ValueTree stream.
    // For this test we use parse_valuetree directly + extract_initial_state
    // through decode_event by handcrafting the FullSync payload.
    let mut payload = vec![0x02];
    write_value_tree(&root, &mut payload);

    let event = decode_event(&payload, &l).unwrap();
    let initial = match event {
        DeviceEvent::InitialState(events) => events,
        other => panic!("expected InitialState, got {other:?}"),
    };

    // 3 fader levels + 3 mute + 1 cue + 26 mix levels = 33 events.
    assert_eq!(initial.len(), 33);

    // First three should be fader-level inits from PHYSICALINTERFACE/FADER.
    assert_eq!(
        initial[0],
        DeviceEvent::FaderLevelChanged {
            fader: Fader::Physical1,
            level: 50,
        }
    );
    assert_eq!(
        initial[1],
        DeviceEvent::FaderLevelChanged {
            fader: Fader::Physical2,
            level: 60,
        }
    );
    assert_eq!(
        initial[2],
        DeviceEvent::FaderLevelChanged {
            fader: Fader::Physical3,
            level: 70,
        }
    );
}

#[test]
fn structural_change_invalidates_layout() {
    let l = layout();
    // childRemoved: [4] [path=[2]] [oldIndex=3]
    let payload = [0x04, 0x01, 0x01, 0x01, 0x02, 0x01, 0x03];
    let event = decode_event(&payload, &l).unwrap();
    assert_eq!(event, DeviceEvent::LayoutInvalidated);
}

#[test]
fn full_round_trip_through_transport_packet() {
    use rodecaster_protocol::Packet;
    let l = layout();
    let cmd = Command::SetFaderMute {
        fader: Fader::Physical1,
        mute: true,
    };
    let payloads = cmd.encode(&l).unwrap();

    // Wrap in transport Packet.
    let pkt = Packet::new(payloads[0].clone());
    let wire = pkt.to_bytes();

    // Unwrap back.
    let (received_pkt, used) = Packet::from_bytes(&wire).unwrap();
    assert_eq!(used, wire.len());

    let event = decode_event(&received_pkt.payload, &l).unwrap();
    assert_eq!(
        event,
        DeviceEvent::FaderMuteChanged {
            fader: Fader::Physical1,
            muted: true,
        }
    );
}

/// Verify the same Command produces different wire bytes when the Layout
/// differs, all the way through the round-trip pipeline. Catches any hidden
/// hardcoded position.
#[test]
fn round_trip_addresses_track_layout_not_constants() {
    // Tree A: CHANNEL at root index 4.
    let phys = nc("PHYSICALINTERFACE", vec![n("FADER"), n("FADER")]);
    let mut a_children = vec![
        phys.clone(),
        n("X"),
        n("Y"),
        n("Z"),
        n("CHANNEL"),
        n("CHANNEL"),
    ];
    for _ in 0..13 {
        a_children.push(n("MIX"));
    }
    let layout_a = Layout::from_full_sync(&nc("DEVICE", a_children)).unwrap();

    // Tree B: CHANNEL at root index 7.
    let mut b_children = vec![
        phys,
        n("X"),
        n("Y"),
        n("Z"),
        n("W1"),
        n("W2"),
        n("W3"),
        n("CHANNEL"),
        n("CHANNEL"),
    ];
    for _ in 0..13 {
        b_children.push(n("MIX"));
    }
    let layout_b = Layout::from_full_sync(&nc("DEVICE", b_children)).unwrap();

    let cmd = Command::SetFaderMute {
        fader: Fader::Physical1,
        mute: true,
    };

    let a = cmd.encode(&layout_a).unwrap();
    let b = cmd.encode(&layout_b).unwrap();
    assert_ne!(a, b, "wire bytes must differ when Layout differs");

    // Both must decode back to fader 0 under their own Layout.
    assert_eq!(
        decode_event(&a[0], &layout_a).unwrap(),
        DeviceEvent::FaderMuteChanged {
            fader: Fader::Physical1,
            muted: true,
        }
    );
    assert_eq!(
        decode_event(&b[0], &layout_b).unwrap(),
        DeviceEvent::FaderMuteChanged {
            fader: Fader::Physical1,
            muted: true,
        }
    );

    // And critically, decoding A's bytes under B's Layout produces a different
    // (or Unknown) result because the addressing context differs.
    let cross = decode_event(&a[0], &layout_b).unwrap();
    assert_ne!(
        cross,
        DeviceEvent::FaderMuteChanged {
            fader: Fader::Physical1,
            muted: true,
        },
        "different Layout must NOT resolve A's path the same way"
    );
}

/// Verify change_frame::decode reads exactly the bytes the encoder writes.
#[test]
fn change_frame_codec_round_trip_via_encoder() {
    use rodecaster_protocol::change_frame::encode_property_changed;

    let cases = vec![
        (vec![28u32], "channelOutputMute", Value::Bool(true)),
        (
            vec![217],
            "mixLevelWithAnchor",
            Value::String("0.5|0.5".to_string()),
        ),
        (vec![0, 7], "faderLevel", Value::Int(75)),
        (vec![0], "rootProperty", Value::Undefined),
    ];

    for (path, name, value) in cases {
        let bytes = encode_property_changed(&path, name, &value);
        let frame = decode_change_frame(&bytes).expect("decodes");
        match frame {
            ChangeFrame::PropertyChanged {
                path: p,
                name: n,
                value: v,
            } => {
                assert_eq!(p, path);
                assert_eq!(n, name);
                assert_eq!(v, value);
            }
            other => panic!("expected PropertyChanged, got {other:?}"),
        }
    }
}

// Helpers

/// Serialize a Node back to JUCE `ValueTree::writeToStream` bytes (name,
/// prop count, props, child count, children, recursive). Mirror of the
/// crate's parse_node. Used by the FullSync integration test to build a
/// fullSync payload from a Node tree.
fn write_value_tree(node: &Node, out: &mut Vec<u8>) {
    out.extend_from_slice(node.name.as_bytes());
    out.push(0);
    write_compressed_int(out, node.properties.len() as i64);
    for p in &node.properties {
        out.extend_from_slice(p.name.as_bytes());
        out.push(0);
        p.value.write_to_stream(out);
    }
    write_compressed_int(out, node.children.len() as i64);
    for c in &node.children {
        write_value_tree(c, out);
    }
}

fn write_compressed_int(out: &mut Vec<u8>, value: i64) {
    let negative = value < 0;
    let mut magnitude = value.unsigned_abs();
    let mut bytes = [0u8; 4];
    let mut num_bytes = 0usize;
    while magnitude != 0 && num_bytes < 4 {
        bytes[num_bytes] = (magnitude & 0xff) as u8;
        magnitude >>= 8;
        num_bytes += 1;
    }
    let size_byte = num_bytes as u8 | if negative { 0x80 } else { 0 };
    out.push(size_byte);
    out.extend_from_slice(&bytes[..num_bytes]);
}
