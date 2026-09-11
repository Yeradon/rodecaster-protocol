use super::*;
use crate::change_frame::encode_property_changed;
use crate::test_fixtures::{layout, n, nc, np, prop};

#[test]
fn decodes_fader_mute_changed() {
    let l = layout();
    let path = l.channel_path(2).unwrap();
    let payload = encode_property_changed(&path, "channelOutputMute", &Value::Bool(true));
    let event = decode_event(&payload, &l).unwrap();
    assert_eq!(
        event,
        DeviceEvent::FaderMuteChanged {
            fader: Fader::Physical3,
            muted: true,
        }
    );
}

#[test]
fn decodes_fader_cue_changed() {
    let l = layout();
    let path = l.channel_path(0).unwrap();
    let payload = encode_property_changed(&path, "channelCueEnable", &Value::Bool(false));
    let event = decode_event(&payload, &l).unwrap();
    assert_eq!(
        event,
        DeviceEvent::FaderCueChanged {
            fader: Fader::Physical1,
            enabled: false,
        }
    );
}

#[test]
fn decodes_fader_level_changed_two_level_path() {
    let l = layout();
    let path = l.fader_path(1).unwrap();
    let payload = encode_property_changed(&path, "faderLevel", &Value::Int(99));
    let event = decode_event(&payload, &l).unwrap();
    assert_eq!(
        event,
        DeviceEvent::FaderLevelChanged {
            fader: Fader::Physical2,
            level: 99,
        }
    );
}

#[test]
fn decodes_fader_level_clamps_to_midi_range() {
    let l = layout();
    let path = l.fader_path(0).unwrap();
    let payload = encode_property_changed(&path, "faderLevel", &Value::Int(500));
    let event = decode_event(&payload, &l).unwrap();
    assert_eq!(
        event,
        DeviceEvent::FaderLevelChanged {
            fader: Fader::Physical1,
            level: 127,
        }
    );
}

#[test]
fn decodes_mix_disabled_changed() {
    let l = layout();
    let path = l.mix_cell_path(1, 5).unwrap();
    let payload = encode_property_changed(&path, "mixDisabled", &Value::Bool(true));
    let event = decode_event(&payload, &l).unwrap();
    assert_eq!(
        event,
        DeviceEvent::MixDisabledChanged {
            source: Source::Combo2,
            mix: MixOutput::Recording,
            disabled: true,
        }
    );
}

#[test]
fn decodes_mix_level_anchor_split() {
    let l = layout();
    let path = l.mix_cell_path(0, 0).unwrap();
    let payload = encode_property_changed(
        &path,
        "mixLevelWithAnchor",
        &Value::String("0.3|0.7".to_string()),
    );
    let event = decode_event(&payload, &l).unwrap();
    match event {
        DeviceEvent::MixLevelChanged {
            source,
            mix,
            anchor,
            value,
        } => {
            assert_eq!(source, Source::Combo1);
            assert_eq!(mix, MixOutput::Headphone1);
            assert!((anchor - 0.3).abs() < 1e-4);
            assert!((value - 0.7).abs() < 1e-4);
        }
        _ => panic!("wrong variant"),
    }
}

#[test]
fn decodes_encoder_signal_as_fader_touch() {
    // encoderSignal addresses the fader by raw index (stride 1, no base).
    let l = layout();
    let payload = encode_property_changed(&[1u32], "encoderSignal", &Value::Int(1));
    let event = decode_event(&payload, &l).unwrap();
    assert_eq!(
        event,
        DeviceEvent::FaderTouched {
            fader: Fader::Physical2
        }
    );
}

#[test]
fn decodes_encoder_colour_palette_index() {
    // encoderColour shares encoderSignal's single-level addressing. >= 0 is
    // the palette index.
    let l = layout();
    let payload = encode_property_changed(&[2u32], "encoderColour", &Value::Int(7));
    let event = decode_event(&payload, &l).unwrap();
    assert_eq!(
        event,
        DeviceEvent::FaderEncoderColourChanged {
            fader: Fader::Physical3,
            colour: Some(7),
        }
    );
}

#[test]
fn decodes_encoder_colour_cleared() {
    // The wire's -1 ("cleared") collapses to `colour: None`.
    let l = layout();
    let payload = encode_property_changed(&[0u32], "encoderColour", &Value::Int(-1));
    let event = decode_event(&payload, &l).unwrap();
    assert_eq!(
        event,
        DeviceEvent::FaderEncoderColourChanged {
            fader: Fader::Physical1,
            colour: None,
        }
    );
}

#[test]
fn decodes_mix_link_request_client_trigger_and_device_ack() {
    // mixLinkRequest at a cell's single-level path; direction = property
    // name, origin = byte[2] (02 = client trigger, 03 = device ack).
    let l = layout();
    let path = l.mix_cell_path(0, 0).unwrap();
    let trigger = encode_property_changed(
        &path,
        "mixLinkRequest",
        &Value::Binary(vec![0x01, 0x01, 0x02, 0x01, 0x01, 0x02]),
    );
    let ack = encode_property_changed(
        &path,
        "mixLinkRequest",
        &Value::Binary(vec![0x01, 0x01, 0x03, 0x01, 0x01, 0x03]),
    );
    let trigger_evt = decode_event(&trigger, &l).unwrap();
    let ack_evt = decode_event(&ack, &l).unwrap();
    let source = Source::from_protocol(0).unwrap();
    let mix = MixOutput::from_protocol(0).unwrap();
    assert_eq!(
        trigger_evt,
        DeviceEvent::MixLinkRequested {
            source,
            mix,
            direction: MixLinkDirection::Link,
            origin: TriggerPhase::Press,
        }
    );
    assert_eq!(
        ack_evt,
        DeviceEvent::MixLinkRequested {
            source,
            mix,
            direction: MixLinkDirection::Link,
            origin: TriggerPhase::Release,
        }
    );
}

#[test]
fn decodes_mix_unlink_request_direction_from_property_name() {
    // mixUnlinkRequest carries the same payload semantics; direction is
    // entirely encoded in the property name.
    let l = layout();
    let path = l.mix_cell_path(0, 0).unwrap();
    let payload = encode_property_changed(
        &path,
        "mixUnlinkRequest",
        &Value::Binary(vec![0x01, 0x01, 0x03, 0x01, 0x01, 0x02]),
    );
    let event = decode_event(&payload, &l).unwrap();
    assert_eq!(
        event,
        DeviceEvent::MixLinkRequested {
            source: Source::from_protocol(0).unwrap(),
            mix: MixOutput::from_protocol(0).unwrap(),
            direction: MixLinkDirection::Unlink,
            origin: TriggerPhase::Release,
        }
    );
}

#[test]
fn decodes_channel_input_source_echo_at_stride_6() {
    // The echo for fader N sits at first_channel + 6*N, not the stride-1
    // write path.
    let l = layout();
    let path = vec![l.first_channel() + 6 * 2];
    let payload = encode_property_changed(&path, "channelInputSource", &Value::Int(7));
    let event = decode_event(&payload, &l).unwrap();
    assert_eq!(
        event,
        DeviceEvent::FaderAssignmentChanged {
            fader: Fader::Physical3,
            source: Some(Source::Usb1),
        }
    );
}

#[test]
fn channel_input_source_negative_value_is_unassigned() {
    let l = layout();
    let path = vec![l.first_channel()]; // fader 0
    let payload = encode_property_changed(&path, "channelInputSource", &Value::Int(-1));
    let event = decode_event(&payload, &l).unwrap();
    assert_eq!(
        event,
        DeviceEvent::FaderAssignmentChanged {
            fader: Fader::Physical1,
            source: None,
        }
    );
}

#[test]
fn unknown_property_preserves_wire_data() {
    let l = layout();
    // A non-addressable path (the root's OTHER child at index 0, before
    // first_channel): no node family claims it, so it must fall to Unknown
    // with its wire data intact. (A property on a CHANNEL path would instead
    // be promoted to ChannelParamChanged: see `decodes_channel_strip_param`.)
    let path = vec![0u32];
    let payload = encode_property_changed(&path, "futureProperty", &Value::Int(42));
    let event = decode_event(&payload, &l).unwrap();
    match event {
        DeviceEvent::Unknown {
            prop_name,
            path: p,
            value,
        } => {
            assert_eq!(prop_name, "futureProperty");
            assert_eq!(p, path);
            assert_eq!(value, Some(Value::Int(42)));
        }
        other => panic!("expected Unknown, got {other:?}"),
    }
}

#[test]
fn decodes_channel_strip_param() {
    // Any non-specialized property on a CHANNEL path is promoted to a typed,
    // fader-resolved channel-strip event carrying the wire value verbatim.
    let l = layout();
    let path = l.channel_path(2).unwrap();
    let payload = encode_property_changed(&path, "eqHighGain", &Value::Double(6.0));
    let event = decode_event(&payload, &l).unwrap();
    assert_eq!(
        event,
        DeviceEvent::ChannelParamChanged {
            fader: Fader::Physical3,
            param: ChannelParam::EqHighGain,
            value: Value::Double(6.0),
        }
    );
}

#[test]
fn decodes_unknown_channel_param_as_other() {
    // An unrecognized property name on a CHANNEL path is still fader-resolved
    // and typed (param = Other), not dropped to Unknown.
    let l = layout();
    let path = l.channel_path(0).unwrap();
    let payload = encode_property_changed(&path, "channelMysteryKnob", &Value::Int(3));
    let event = decode_event(&payload, &l).unwrap();
    assert_eq!(
        event,
        DeviceEvent::ChannelParamChanged {
            fader: Fader::Physical1,
            param: ChannelParam::Other("channelMysteryKnob".to_string()),
            value: Value::Int(3),
        }
    );
}

#[test]
fn decodes_input_source_param() {
    // A property on an INPUTSOURCE path resolves to a Source (not a Fader)
    // and a typed InputSourceParam, carrying the wire value verbatim.
    let l = layout();
    let path = l.input_source_path(1).unwrap(); // ordinal 1 == Combo2
    let payload = encode_property_changed(&path, "inputPower", &Value::Int(1));
    let event = decode_event(&payload, &l).unwrap();
    assert_eq!(
        event,
        DeviceEvent::InputSourceParamChanged {
            source: Source::Combo2,
            param: InputSourceParam::InputPower,
            value: Value::Int(1),
        }
    );
}

#[test]
fn decodes_unknown_input_source_param_as_other() {
    let l = layout();
    let path = l.input_source_path(0).unwrap(); // ordinal 0 == Combo1
    let payload = encode_property_changed(&path, "inputMysteryFlag", &Value::Bool(true));
    let event = decode_event(&payload, &l).unwrap();
    assert_eq!(
        event,
        DeviceEvent::InputSourceParamChanged {
            source: Source::Combo1,
            param: InputSourceParam::Other("inputMysteryFlag".to_string()),
            value: Value::Bool(true),
        }
    );
}

#[test]
fn decodes_master_param() {
    // A property on the MASTERCHANNEL path resolves to a key-less, typed
    // MasterParam carrying the wire value verbatim.
    let l = layout();
    let path = l.master_channel_path().unwrap();
    let payload = encode_property_changed(&path, "masterCompellorOn", &Value::Bool(true));
    let event = decode_event(&payload, &l).unwrap();
    assert_eq!(
        event,
        DeviceEvent::MasterParamChanged {
            param: MasterParam::CompellorOn,
            value: Value::Bool(true),
        }
    );
}

#[test]
fn decodes_output_param() {
    let l = layout();
    let path = l.output_path().unwrap();
    let payload = encode_property_changed(&path, "outputMonLevel", &Value::Double(0.0));
    let event = decode_event(&payload, &l).unwrap();
    assert_eq!(
        event,
        DeviceEvent::OutputParamChanged {
            param: OutputParam::MonLevel,
            value: Value::Double(0.0),
        }
    );
}

#[test]
fn decodes_unknown_master_param_as_other() {
    let l = layout();
    let path = l.master_channel_path().unwrap();
    let payload = encode_property_changed(&path, "masterMysteryFlag", &Value::Int(2));
    let event = decode_event(&payload, &l).unwrap();
    assert_eq!(
        event,
        DeviceEvent::MasterParamChanged {
            param: MasterParam::Other("masterMysteryFlag".to_string()),
            value: Value::Int(2),
        }
    );
}

#[test]
fn decodes_ducker_param() {
    let l = layout();
    let path = l.ducker_path().unwrap();
    let payload = encode_property_changed(&path, "duckerDepth", &Value::Double(-9.0));
    let event = decode_event(&payload, &l).unwrap();
    assert_eq!(
        event,
        DeviceEvent::DuckerParamChanged {
            param: DuckerParam::Depth,
            value: Value::Double(-9.0),
        }
    );
}

#[test]
fn decodes_recorder_param() {
    let l = layout();
    let path = l.recorder_path().unwrap();
    // The request* write channel echoes back as a typed event too.
    let payload = encode_property_changed(&path, "requestRecordState", &Value::Int(3));
    let event = decode_event(&payload, &l).unwrap();
    assert_eq!(
        event,
        DeviceEvent::RecorderParamChanged {
            param: RecorderParam::RequestRecordState,
            value: Value::Int(3),
        }
    );
}

#[test]
fn decodes_player_param() {
    let l = layout();
    let path = l.player_path().unwrap();
    let payload = encode_property_changed(&path, "playerState", &Value::Int(0));
    let event = decode_event(&payload, &l).unwrap();
    assert_eq!(
        event,
        DeviceEvent::PlayerParamChanged {
            param: PlayerParam::State,
            value: Value::Int(0),
        }
    );
}

#[test]
fn decodes_headphone_param_carries_index() {
    // A property on the second HEADPHONE node resolves to headphone index 1.
    let l = layout();
    let path = l.headphone_path(1).unwrap();
    let payload =
        encode_property_changed(&path, "headphoneColour", &Value::String("ffd43580".into()));
    let event = decode_event(&payload, &l).unwrap();
    assert_eq!(
        event,
        DeviceEvent::HeadphoneParamChanged {
            headphone: 1,
            param: HeadphoneParam::Colour,
            value: Value::String("ffd43580".into()),
        }
    );
}

#[test]
fn decodes_effects_param_carries_index() {
    // A property on the third EFFECTS_PARAMETERS node resolves to slot index 2.
    let l = layout();
    let path = l.effects_path(2).unwrap();
    let payload = encode_property_changed(&path, "reverbMix", &Value::Double(0.4));
    let event = decode_event(&payload, &l).unwrap();
    assert_eq!(
        event,
        DeviceEvent::EffectsParamChanged {
            effects: 2,
            param: EffectsParam::ReverbMix,
            value: Value::Double(0.4),
        }
    );
}

#[test]
fn decodes_unknown_effects_param_as_other() {
    let l = layout();
    let path = l.effects_path(0).unwrap();
    let payload = encode_property_changed(&path, "flangerOn", &Value::Bool(true));
    let event = decode_event(&payload, &l).unwrap();
    assert_eq!(
        event,
        DeviceEvent::EffectsParamChanged {
            effects: 0,
            param: EffectsParam::Other("flangerOn".to_string()),
            value: Value::Bool(true),
        }
    );
}

#[test]
fn decodes_gui_param() {
    // A property on the GUI path resolves to a key-less, typed GuiParam.
    let l = layout();
    let path = l.gui_path().unwrap();
    let payload = encode_property_changed(&path, "screenBrightness", &Value::Int(250));
    let event = decode_event(&payload, &l).unwrap();
    assert_eq!(
        event,
        DeviceEvent::GuiParamChanged {
            param: GuiParam::ScreenBrightness,
            value: Value::Int(250),
        }
    );
}

#[test]
fn decodes_unknown_gui_param_as_other() {
    let l = layout();
    let path = l.gui_path().unwrap();
    let payload = encode_property_changed(&path, "mysteryGuiFlag", &Value::Bool(true));
    let event = decode_event(&payload, &l).unwrap();
    assert_eq!(
        event,
        DeviceEvent::GuiParamChanged {
            param: GuiParam::Other("mysteryGuiFlag".to_string()),
            value: Value::Bool(true),
        }
    );
}

#[test]
fn decodes_system_param() {
    // A property on the SYSTEM path resolves to a key-less, typed SystemParam.
    let l = layout();
    let path = l.system_path().unwrap();
    let payload = encode_property_changed(
        &path,
        "systemFirmwareVersion",
        &Value::String("1.7.3".into()),
    );
    let event = decode_event(&payload, &l).unwrap();
    assert_eq!(
        event,
        DeviceEvent::SystemParamChanged {
            param: SystemParam::FirmwareVersion,
            value: Value::String("1.7.3".into()),
        }
    );
}

#[test]
fn decodes_unknown_system_param_as_other() {
    let l = layout();
    let path = l.system_path().unwrap();
    let payload = encode_property_changed(&path, "mysterySystemFlag", &Value::Bool(true));
    let event = decode_event(&payload, &l).unwrap();
    assert_eq!(
        event,
        DeviceEvent::SystemParamChanged {
            param: SystemParam::Other("mysterySystemFlag".to_string()),
            value: Value::Bool(true),
        }
    );
}

#[test]
fn decodes_pad_param_carries_index() {
    // A pad* property on a PAD path resolves to a typed PadParam carrying the
    // pad's ordinal index within the SOUNDPADS run.
    let l = layout();
    let path = l.pad_path(2).unwrap();
    let payload = encode_property_changed(&path, "padColourIndex", &Value::Int(7));
    let event = decode_event(&payload, &l).unwrap();
    assert_eq!(
        event,
        DeviceEvent::PadParamChanged {
            pad: 2,
            param: PadParam::ColourIndex,
            value: Value::Int(7),
        }
    );
}

#[test]
fn decodes_unknown_pad_param_as_other() {
    let l = layout();
    let path = l.pad_path(0).unwrap();
    let payload = encode_property_changed(&path, "padMysteryFlag", &Value::Bool(true));
    let event = decode_event(&payload, &l).unwrap();
    assert_eq!(
        event,
        DeviceEvent::PadParamChanged {
            pad: 0,
            param: PadParam::Other("padMysteryFlag".to_string()),
            value: Value::Bool(true),
        }
    );
}

#[test]
fn decodes_unknown_player_param_as_other() {
    let l = layout();
    let path = l.player_path().unwrap();
    let payload = encode_property_changed(&path, "playerMysteryFlag", &Value::Int(7));
    let event = decode_event(&payload, &l).unwrap();
    assert_eq!(
        event,
        DeviceEvent::PlayerParamChanged {
            param: PlayerParam::Other("playerMysteryFlag".to_string()),
            value: Value::Int(7),
        }
    );
}

#[test]
fn channel_mute_takes_precedence_over_generic_param() {
    // channelOutputMute keeps its dedicated FaderMuteChanged variant even
    // though it lives on the same CHANNEL path as the strip params.
    let l = layout();
    let path = l.channel_path(1).unwrap();
    let payload = encode_property_changed(&path, "channelOutputMute", &Value::Bool(true));
    let event = decode_event(&payload, &l).unwrap();
    assert_eq!(
        event,
        DeviceEvent::FaderMuteChanged {
            fader: Fader::Physical2,
            muted: true,
        }
    );
}

#[test]
fn extract_initial_state_includes_channel_strip_params() {
    let phys = nc(
        "PHYSICALINTERFACE",
        vec![np("FADER", vec![prop("faderLevel", Value::Int(10))])],
    );
    let mut children = vec![
        phys,
        np(
            "CHANNEL",
            vec![
                prop("channelOutputMute", Value::Bool(false)),
                prop("eqOn", Value::Bool(true)),
                prop("compressorThreshold", Value::Double(-18.0)),
            ],
        ),
    ];
    for _ in 0..13 {
        children.push(n("MIX"));
    }
    let root = nc("DEVICE", children);
    let l = Layout::from_full_sync(&root).unwrap();
    let events = extract_initial_state(&root, &l);

    // mute keeps its dedicated variant; the two strip props are typed params.
    assert!(events.contains(&DeviceEvent::FaderMuteChanged {
        fader: Fader::Physical1,
        muted: false,
    }));
    assert!(events.contains(&DeviceEvent::ChannelParamChanged {
        fader: Fader::Physical1,
        param: ChannelParam::EqOn,
        value: Value::Bool(true),
    }));
    assert!(events.contains(&DeviceEvent::ChannelParamChanged {
        fader: Fader::Physical1,
        param: ChannelParam::CompressorThreshold,
        value: Value::Double(-18.0),
    }));
}

#[test]
fn extract_initial_state_includes_input_source_params() {
    let phys = nc("PHYSICALINTERFACE", vec![n("FADER")]);
    let mut children = vec![phys, n("CHANNEL")];
    for _ in 0..13 {
        children.push(n("MIX"));
    }
    // Two INPUTSOURCE nodes -> ordinals 0 (Combo1) and 1 (Combo2).
    children.push(np(
        "INPUTSOURCE",
        vec![
            prop("inputId", Value::Int(0)),
            prop("inputPower", Value::Int(1)),
        ],
    ));
    children.push(np(
        "INPUTSOURCE",
        vec![prop("inputMicrophoneGain", Value::Int(50))],
    ));
    let root = nc("DEVICE", children);
    let l = Layout::from_full_sync(&root).unwrap();
    let events = extract_initial_state(&root, &l);

    assert!(events.contains(&DeviceEvent::InputSourceParamChanged {
        source: Source::Combo1,
        param: InputSourceParam::InputId,
        value: Value::Int(0),
    }));
    assert!(events.contains(&DeviceEvent::InputSourceParamChanged {
        source: Source::Combo1,
        param: InputSourceParam::InputPower,
        value: Value::Int(1),
    }));
    assert!(events.contains(&DeviceEvent::InputSourceParamChanged {
        source: Source::Combo2,
        param: InputSourceParam::InputMicrophoneGain,
        value: Value::Int(50),
    }));
}

#[test]
fn extract_initial_state_includes_master_and_output_params() {
    let phys = nc("PHYSICALINTERFACE", vec![n("FADER")]);
    let mut children = vec![phys, n("CHANNEL")];
    for _ in 0..13 {
        children.push(n("MIX"));
    }
    children.push(np(
        "MASTERCHANNEL",
        vec![
            prop("masterCompellorOn", Value::Bool(true)),
            prop("masterDelaySeconds", Value::Double(0.0)),
        ],
    ));
    children.push(np(
        "OUTPUT",
        vec![
            prop("outputMonLevel", Value::Double(0.0)),
            prop("outputMysteryFlag", Value::Int(9)),
        ],
    ));
    let root = nc("DEVICE", children);
    let l = Layout::from_full_sync(&root).unwrap();
    let events = extract_initial_state(&root, &l);

    assert!(events.contains(&DeviceEvent::MasterParamChanged {
        param: MasterParam::CompellorOn,
        value: Value::Bool(true),
    }));
    assert!(events.contains(&DeviceEvent::MasterParamChanged {
        param: MasterParam::DelaySeconds,
        value: Value::Double(0.0),
    }));
    assert!(events.contains(&DeviceEvent::OutputParamChanged {
        param: OutputParam::MonLevel,
        value: Value::Double(0.0),
    }));
    // Untyped property still surfaces, typed as Other (never dropped).
    assert!(events.contains(&DeviceEvent::OutputParamChanged {
        param: OutputParam::Other("outputMysteryFlag".to_string()),
        value: Value::Int(9),
    }));
}

#[test]
fn extract_initial_state_includes_ducker_recorder_player_headphone() {
    let phys = nc("PHYSICALINTERFACE", vec![n("FADER")]);
    let mut children = vec![phys, n("CHANNEL")];
    for _ in 0..13 {
        children.push(n("MIX"));
    }
    children.push(np("DUCKER", vec![prop("duckerDepth", Value::Double(-9.0))]));
    children.push(np("RECORDER", vec![prop("recordState", Value::Int(0))]));
    children.push(np("PLAYER", vec![prop("playerState", Value::Int(0))]));
    // Two HEADPHONE nodes -> indices 0 and 1.
    children.push(np("HEADPHONE", vec![prop("headphoneType", Value::Int(0))]));
    children.push(np(
        "HEADPHONE",
        vec![prop("headphoneColour", Value::String("ffd43580".into()))],
    ));
    let root = nc("DEVICE", children);
    let l = Layout::from_full_sync(&root).unwrap();
    let events = extract_initial_state(&root, &l);

    assert!(events.contains(&DeviceEvent::DuckerParamChanged {
        param: DuckerParam::Depth,
        value: Value::Double(-9.0),
    }));
    assert!(events.contains(&DeviceEvent::RecorderParamChanged {
        param: RecorderParam::RecordState,
        value: Value::Int(0),
    }));
    assert!(events.contains(&DeviceEvent::PlayerParamChanged {
        param: PlayerParam::State,
        value: Value::Int(0),
    }));
    // Each headphone keeps its own index.
    assert!(events.contains(&DeviceEvent::HeadphoneParamChanged {
        headphone: 0,
        param: HeadphoneParam::Type,
        value: Value::Int(0),
    }));
    assert!(events.contains(&DeviceEvent::HeadphoneParamChanged {
        headphone: 1,
        param: HeadphoneParam::Colour,
        value: Value::String("ffd43580".into()),
    }));
}

#[test]
fn extract_initial_state_includes_effects_params() {
    let phys = nc("PHYSICALINTERFACE", vec![n("FADER")]);
    let mut children = vec![phys, n("CHANNEL")];
    for _ in 0..13 {
        children.push(n("MIX"));
    }
    // Two EFFECTS_PARAMETERS nodes -> slot indices 0 and 1.
    children.push(np(
        "EFFECTS_PARAMETERS",
        vec![
            prop("effectsIdx", Value::Int(0)),
            prop("reverbOn", Value::Bool(true)),
        ],
    ));
    children.push(np(
        "EFFECTS_PARAMETERS",
        vec![prop("echoMix", Value::Double(0.25))],
    ));
    let root = nc("DEVICE", children);
    let l = Layout::from_full_sync(&root).unwrap();
    let events = extract_initial_state(&root, &l);

    assert!(events.contains(&DeviceEvent::EffectsParamChanged {
        effects: 0,
        param: EffectsParam::Idx,
        value: Value::Int(0),
    }));
    assert!(events.contains(&DeviceEvent::EffectsParamChanged {
        effects: 0,
        param: EffectsParam::ReverbOn,
        value: Value::Bool(true),
    }));
    // The second slot keeps its own index.
    assert!(events.contains(&DeviceEvent::EffectsParamChanged {
        effects: 1,
        param: EffectsParam::EchoMix,
        value: Value::Double(0.25),
    }));
}

#[test]
fn extract_initial_state_includes_gui_params() {
    let phys = nc("PHYSICALINTERFACE", vec![n("FADER")]);
    let mut children = vec![phys, n("CHANNEL")];
    for _ in 0..13 {
        children.push(n("MIX"));
    }
    children.push(np(
        "GUI",
        vec![
            prop("lang", Value::String("en".into())),
            prop("screenBrightness", Value::Int(250)),
            prop("eqParamModeLow", Value::Int(0)),
        ],
    ));
    let root = nc("DEVICE", children);
    let l = Layout::from_full_sync(&root).unwrap();
    let events = extract_initial_state(&root, &l);

    assert!(events.contains(&DeviceEvent::GuiParamChanged {
        param: GuiParam::Lang,
        value: Value::String("en".into()),
    }));
    assert!(events.contains(&DeviceEvent::GuiParamChanged {
        param: GuiParam::ScreenBrightness,
        value: Value::Int(250),
    }));
    // eqParamMode* belong to GUI, not the channel strip.
    assert!(events.contains(&DeviceEvent::GuiParamChanged {
        param: GuiParam::EqParamModeLow,
        value: Value::Int(0),
    }));
}

#[test]
fn extract_initial_state_includes_system_params() {
    let phys = nc("PHYSICALINTERFACE", vec![n("FADER")]);
    let mut children = vec![phys, n("CHANNEL")];
    for _ in 0..13 {
        children.push(n("MIX"));
    }
    children.push(np(
        "SYSTEM",
        vec![
            // boardType Int(0) keeps the detected model Pro II and surfaces as
            // the typed BoardType variant.
            prop("boardType", Value::Int(0)),
            prop("systemFirmwareVersion", Value::String("1.7.3".into())),
            // Quirky firmware casing must still type-resolve, not fall to Other.
            prop("lastRecordingID", Value::Int(42)),
            prop("powerOffRequest", Value::Bool(false)),
        ],
    ));
    let root = nc("DEVICE", children);
    let l = Layout::from_full_sync(&root).unwrap();
    let events = extract_initial_state(&root, &l);

    assert!(events.contains(&DeviceEvent::SystemParamChanged {
        param: SystemParam::BoardType,
        value: Value::Int(0),
    }));
    assert!(events.contains(&DeviceEvent::SystemParamChanged {
        param: SystemParam::FirmwareVersion,
        value: Value::String("1.7.3".into()),
    }));
    assert!(events.contains(&DeviceEvent::SystemParamChanged {
        param: SystemParam::LastRecordingId,
        value: Value::Int(42),
    }));
    // The powerOffRequest readback surfaces as a typed SystemParam, distinct
    // from the dedicated Command::PowerOff write frame.
    assert!(events.contains(&DeviceEvent::SystemParamChanged {
        param: SystemParam::PowerOffRequest,
        value: Value::Bool(false),
    }));
}

#[test]
fn extract_initial_state_includes_pad_params() {
    let phys = nc("PHYSICALINTERFACE", vec![n("FADER")]);
    let mut children = vec![phys, n("CHANNEL")];
    for _ in 0..13 {
        children.push(n("MIX"));
    }
    children.push(nc(
        "SOUNDPADS",
        vec![
            np(
                "PAD",
                vec![
                    prop("padIdx", Value::Int(0)),
                    prop("padName", Value::String("Applause".into())),
                ],
            ),
            np(
                "PAD",
                vec![
                    prop("padIdx", Value::Int(1)),
                    prop("padColourIndex", Value::Int(7)),
                ],
            ),
        ],
    ));
    let root = nc("DEVICE", children);
    let l = Layout::from_full_sync(&root).unwrap();
    let events = extract_initial_state(&root, &l);

    // Pad 0 carries its identity + name.
    assert!(events.contains(&DeviceEvent::PadParamChanged {
        pad: 0,
        param: PadParam::Idx,
        value: Value::Int(0),
    }));
    assert!(events.contains(&DeviceEvent::PadParamChanged {
        pad: 0,
        param: PadParam::Name,
        value: Value::String("Applause".into()),
    }));
    // Pad 1 is a distinct index in the same run.
    assert!(events.contains(&DeviceEvent::PadParamChanged {
        pad: 1,
        param: PadParam::ColourIndex,
        value: Value::Int(7),
    }));
}

#[test]
fn structural_change_yields_layout_invalidated() {
    let l = layout();
    // childRemoved: [4] [path=[2]] [oldIndex=3]
    let payload = [0x04, 0x01, 0x01, 0x01, 0x02, 0x01, 0x03];
    let event = decode_event(&payload, &l).unwrap();
    assert_eq!(event, DeviceEvent::LayoutInvalidated);
}

#[test]
fn full_sync_yields_initial_state_with_extracted_events() {
    let phys = nc(
        "PHYSICALINTERFACE",
        vec![
            np("FADER", vec![prop("faderLevel", Value::Int(64))]),
            np("FADER", vec![prop("faderLevel", Value::Int(100))]),
        ],
    );
    let mut children = vec![
        phys,
        np(
            "CHANNEL",
            vec![
                prop("channelOutputMute", Value::Bool(false)),
                prop("channelCueEnable", Value::Bool(true)),
            ],
        ),
        np(
            "CHANNEL",
            vec![prop("channelOutputMute", Value::Bool(true))],
        ),
    ];
    // 13 MIX nodes -> 1 source, mix 0..12.
    for i in 0..13 {
        children.push(np(
            "MIX",
            vec![prop(
                "mixLevelWithAnchor",
                Value::String(format!("0.5|{}", 0.1 * i as f32)),
            )],
        ));
    }
    let root = nc("DEVICE", children);
    let l = Layout::from_full_sync(&root).unwrap();

    let events = extract_initial_state(&root, &l);
    // 2 fader levels + 2 mute + 1 cue + 13 mix levels = 18 events
    assert_eq!(events.len(), 18);
    assert_eq!(
        events[0],
        DeviceEvent::FaderLevelChanged {
            fader: Fader::Physical1,
            level: 64,
        }
    );
    assert_eq!(
        events[1],
        DeviceEvent::FaderLevelChanged {
            fader: Fader::Physical2,
            level: 100,
        }
    );
    assert_eq!(
        events[2],
        DeviceEvent::FaderMuteChanged {
            fader: Fader::Physical1,
            muted: false,
        }
    );
    assert_eq!(
        events[3],
        DeviceEvent::FaderCueChanged {
            fader: Fader::Physical1,
            enabled: true,
        }
    );
    assert_eq!(
        events[4],
        DeviceEvent::FaderMuteChanged {
            fader: Fader::Physical2,
            muted: true,
        }
    );
    // Last few should be mix levels
    if let DeviceEvent::MixLevelChanged { source, mix, .. } = events[17] {
        assert_eq!(source, Source::Combo1);
        assert_eq!(mix, MixOutput::CallMe3);
    } else {
        panic!("expected MixLevelChanged at end");
    }
}

#[test]
fn decodes_mix_minuses_param_changed_with_index() {
    let phys = nc("PHYSICALINTERFACE", vec![n("FADER")]);
    let mut children = vec![phys, n("CHANNEL")];
    for _ in 0..13 {
        children.push(n("MIX"));
    }
    children.push(n("MIXMINUSES"));
    let l = Layout::from_full_sync(&nc("DEVICE", children)).unwrap();
    let path = l.mix_minuses_path(0).unwrap();
    let payload = encode_property_changed(&path, "outputMixMinus", &Value::Bool(true));
    let event = decode_event(&payload, &l).unwrap();
    assert_eq!(
        event,
        DeviceEvent::MixMinusesParamChanged {
            minuses: 0,
            param: MixMinusesParam::OutputMixMinus,
            value: Value::Bool(true),
        }
    );
}

#[test]
fn decodes_rcsync_mix_param_changed_with_index() {
    let phys = nc("PHYSICALINTERFACE", vec![n("FADER")]);
    let mut children = vec![phys, n("CHANNEL")];
    for _ in 0..13 {
        children.push(n("MIX"));
    }
    children.push(n("RCSYNCMIX"));
    let l = Layout::from_full_sync(&nc("DEVICE", children)).unwrap();
    let path = l.rcsync_mix_path(0).unwrap();
    let payload = encode_property_changed(&path, "mixMute", &Value::Bool(true));
    let event = decode_event(&payload, &l).unwrap();
    assert_eq!(
        event,
        DeviceEvent::RcSyncMixParamChanged {
            mix: 0,
            param: RcSyncMixParam::MixMute,
            value: Value::Bool(true),
        }
    );
}

#[test]
fn decode_event_from_frame_matches_decode_event() {
    let l = layout();
    let path = l.channel_path(0).unwrap();
    let payload = encode_property_changed(&path, "channelOutputMute", &Value::Bool(true));
    let frame = crate::change_frame::decode(&payload).unwrap();
    let event_from_frame = decode_event_from_frame(frame, &l);
    let event_from_bytes = decode_event(&payload, &l).unwrap();
    assert_eq!(event_from_frame, event_from_bytes);
    assert_eq!(
        event_from_frame,
        DeviceEvent::FaderMuteChanged {
            fader: Fader::Physical1,
            muted: true,
        }
    );
}
