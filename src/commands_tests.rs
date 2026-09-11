use super::*;
use crate::change_frame::{decode, ChangeFrame};
use crate::test_fixtures::{layout, minimal_layout};
use crate::valuetree::Node;

fn decode_single_prop(bytes: &[u8]) -> (Vec<u32>, String, Value) {
    match decode(bytes).expect("decodes") {
        ChangeFrame::PropertyChanged { path, name, value } => (path, name, value),
        other => panic!("expected PropertyChanged, got {other:?}"),
    }
}

#[test]
fn set_fader_mute_encodes_juce_property_changed() {
    let l = layout();
    // Pro II model (empty SYSTEM node -> default): Physical2 -> fader index 1.
    let bytes = Command::SetFaderMute {
        fader: Fader::Physical2,
        mute: true,
    }
    .encode(&l)
    .unwrap();
    assert_eq!(bytes.len(), 1);

    let (path, name, value) = decode_single_prop(&bytes[0]);
    assert_eq!(path, l.channel_path(1).unwrap());
    assert_eq!(name, "channelOutputMute");
    assert_eq!(value, Value::Bool(true));
}

#[test]
fn set_fader_level_uses_two_level_path_through_physical_interface() {
    let l = layout();
    let bytes = Command::SetFaderLevel {
        fader: Fader::Physical3,
        level: 75,
    }
    .encode(&l)
    .unwrap();
    let (path, name, value) = decode_single_prop(&bytes[0]);
    assert_eq!(path, vec![1, 3]);
    assert_eq!(name, "faderLevel");
    assert_eq!(value, Value::Int(75));
}

#[test]
fn assign_fader_source_some_and_none() {
    let l = layout();

    let some = Command::AssignFaderSource {
        fader: Fader::Physical1,
        source: Some(Source::Combo2_3), // protocol index 5
    }
    .encode(&l)
    .unwrap();
    let frame = decode(&some[0]).unwrap();
    match frame {
        ChangeFrame::PropertyChanged { name, value, .. } => {
            assert_eq!(name, "channelInputSource");
            assert_eq!(value, Value::Int(5));
        }
        _ => panic!("wrong variant"),
    }

    let none = Command::AssignFaderSource {
        fader: Fader::Physical1,
        source: None,
    }
    .encode(&l)
    .unwrap();
    let frame = decode(&none[0]).unwrap();
    match frame {
        ChangeFrame::PropertyChanged { value, .. } => {
            // `-1` as i32, sign-extended through i64.
            assert_eq!(value, Value::Int(CHANNEL_INPUT_SOURCE_UNASSIGNED));
        }
        _ => panic!("wrong variant"),
    }
}

#[test]
fn set_mix_disabled_addresses_source_major_cell() {
    let l = layout();
    let bytes = Command::SetMixDisabled {
        source: Source::Combo2,    // protocol index 1
        mix: MixOutput::Recording, // protocol index 5
        disabled: true,
    }
    .encode(&l)
    .unwrap();
    let frame = decode(&bytes[0]).unwrap();
    match frame {
        ChangeFrame::PropertyChanged { path, name, value } => {
            assert_eq!(path, l.mix_cell_path(1, 5).unwrap());
            assert_eq!(name, "mixDisabled");
            assert_eq!(value, Value::Bool(true));
        }
        _ => panic!("wrong variant"),
    }
}

#[test]
fn link_mix_emits_enable_unmute_trigger() {
    let l = layout();
    let bytes = Command::LinkMix {
        source: Source::Combo1,
        mix: MixOutput::Headphone4,
    }
    .encode(&l)
    .unwrap();
    assert_eq!(
        bytes.len(),
        3,
        "link emits enable + unmute + mixLinkRequest trigger"
    );

    let expect = [
        ("mixDisabled", Value::Bool(false)),
        ("mixMute", Value::Bool(false)),
        ("mixLinkRequest", Value::Binary(MIX_LINK_TRIGGER.to_vec())),
    ];
    for (raw, (exp_name, exp_val)) in bytes.iter().zip(expect.iter()) {
        match decode(raw).unwrap() {
            ChangeFrame::PropertyChanged { name, value, .. } => {
                assert_eq!(&name, exp_name);
                assert_eq!(&value, exp_val);
            }
            _ => panic!("wrong variant"),
        }
    }
}

#[test]
fn unlink_mix_emits_single_trigger() {
    let l = layout();
    let bytes = Command::UnlinkMix {
        source: Source::Combo1,
        mix: MixOutput::Headphone4,
    }
    .encode(&l)
    .unwrap();
    assert_eq!(bytes.len(), 1, "unlink emits mixUnlinkRequest trigger only");

    match decode(&bytes[0]).unwrap() {
        ChangeFrame::PropertyChanged { name, value, .. } => {
            assert_eq!(name, "mixUnlinkRequest");
            assert_eq!(value, Value::Binary(MIX_LINK_TRIGGER.to_vec()));
        }
        _ => panic!("wrong variant"),
    }
}

#[test]
fn set_mix_mute_addresses_cell() {
    let l = layout();
    let bytes = Command::SetMixMute {
        source: Source::Combo2,    // protocol index 1
        mix: MixOutput::Recording, // protocol index 5
        mute: true,
    }
    .encode(&l)
    .unwrap();
    assert_eq!(bytes.len(), 1);
    let frame = decode(&bytes[0]).unwrap();
    match frame {
        ChangeFrame::PropertyChanged { path, name, value } => {
            assert_eq!(path, l.mix_cell_path(1, 5).unwrap());
            assert_eq!(name, "mixMute");
            assert_eq!(value, Value::Bool(true));
        }
        _ => panic!("wrong variant"),
    }
}

#[test]
fn set_channel_param_addresses_channel_path_with_caller_value() {
    let l = layout();
    // Physical3 -> channel index 2; a typed EQ param carrying a Double.
    let bytes = Command::SetChannelParam {
        fader: Fader::Physical3,
        param: ChannelParam::EqHighGain,
        value: Value::Double(6.0),
    }
    .encode(&l)
    .unwrap();
    assert_eq!(bytes.len(), 1);
    let frame = decode(&bytes[0]).unwrap();
    match frame {
        ChangeFrame::PropertyChanged { path, name, value } => {
            assert_eq!(path, l.channel_path(2).unwrap());
            assert_eq!(name, "eqHighGain");
            assert_eq!(value, Value::Double(6.0));
        }
        _ => panic!("wrong variant"),
    }
}

#[test]
fn set_channel_param_other_emits_raw_name() {
    let l = layout();
    // An untyped param still encodes: the crate owns the path, the caller
    // owns the (here raw) name and value.
    let bytes = Command::SetChannelParam {
        fader: Fader::Physical1,
        param: ChannelParam::Other("channelMysteryKnob".to_string()),
        value: Value::Int(3),
    }
    .encode(&l)
    .unwrap();
    let frame = decode(&bytes[0]).unwrap();
    match frame {
        ChangeFrame::PropertyChanged { path, name, value } => {
            assert_eq!(path, l.channel_path(0).unwrap());
            assert_eq!(name, "channelMysteryKnob");
            assert_eq!(value, Value::Int(3));
        }
        _ => panic!("wrong variant"),
    }
}

#[test]
fn set_input_source_param_addresses_input_source_path() {
    let l = layout();
    // Combo2 -> source ordinal 1 -> the 2nd INPUTSOURCE node; a typed preamp
    // param carrying an Int (48V power on).
    let bytes = Command::SetInputSourceParam {
        source: Source::Combo2,
        param: InputSourceParam::InputPower,
        value: Value::Int(1),
    }
    .encode(&l)
    .unwrap();
    assert_eq!(bytes.len(), 1);
    let frame = decode(&bytes[0]).unwrap();
    match frame {
        ChangeFrame::PropertyChanged { path, name, value } => {
            assert_eq!(path, l.input_source_path(1).unwrap());
            assert_eq!(name, "inputPower");
            assert_eq!(value, Value::Int(1));
        }
        _ => panic!("wrong variant"),
    }
}

#[test]
fn set_input_source_param_other_emits_raw_name() {
    let l = layout();
    let bytes = Command::SetInputSourceParam {
        source: Source::Combo1,
        param: InputSourceParam::Other("inputMysteryFlag".to_string()),
        value: Value::Bool(true),
    }
    .encode(&l)
    .unwrap();
    let frame = decode(&bytes[0]).unwrap();
    match frame {
        ChangeFrame::PropertyChanged { path, name, value } => {
            assert_eq!(path, l.input_source_path(0).unwrap());
            assert_eq!(name, "inputMysteryFlag");
            assert_eq!(value, Value::Bool(true));
        }
        _ => panic!("wrong variant"),
    }
}

#[test]
fn set_input_source_param_absent_family_returns_error_not_panic() {
    // A layout built from a tree with no INPUTSOURCE run: encoding must error
    // (bound 0), never panic or misroute onto a CHANNEL/MIX path.
    fn nc(name: &str, children: Vec<Node>) -> Node {
        Node {
            name: name.to_string(),
            properties: vec![],
            children,
        }
    }
    fn n(name: &str) -> Node {
        nc(name, vec![])
    }
    let phys = nc("PHYSICALINTERFACE", vec![n("FADER")]);
    let mut children = vec![phys, n("CHANNEL")];
    for _ in 0..13 {
        children.push(n("MIX"));
    }
    let l = Layout::from_full_sync(&nc("DEVICE", children)).unwrap();
    let err = Command::SetInputSourceParam {
        source: Source::Combo1,
        param: InputSourceParam::InputPower,
        value: Value::Int(1),
    }
    .encode(&l)
    .unwrap_err();
    match err {
        EncodeError::OutOfRange { what, bound, .. } => {
            assert_eq!(what, "input source");
            assert_eq!(bound, 0);
        }
        _ => panic!("wrong error variant"),
    }
}

#[test]
fn set_master_param_addresses_master_channel_path() {
    let l = layout();
    // No addressing key: a typed master param carrying a Double (Compellor
    // threshold). Path must be the single MASTERCHANNEL node.
    let bytes = Command::SetMasterParam {
        param: MasterParam::CompellorThreshold,
        value: Value::Double(0.67),
    }
    .encode(&l)
    .unwrap();
    assert_eq!(bytes.len(), 1);
    let frame = decode(&bytes[0]).unwrap();
    match frame {
        ChangeFrame::PropertyChanged { path, name, value } => {
            assert_eq!(path, l.master_channel_path().unwrap());
            assert_eq!(name, "masterCompellorThreshold");
            assert_eq!(value, Value::Double(0.67));
        }
        _ => panic!("wrong variant"),
    }
}

#[test]
fn set_output_param_addresses_output_path() {
    let l = layout();
    // Monitor mute, a Bool, on the single OUTPUT node.
    let bytes = Command::SetOutputParam {
        param: OutputParam::MonMute,
        value: Value::Bool(true),
    }
    .encode(&l)
    .unwrap();
    assert_eq!(bytes.len(), 1);
    let frame = decode(&bytes[0]).unwrap();
    match frame {
        ChangeFrame::PropertyChanged { path, name, value } => {
            assert_eq!(path, l.output_path().unwrap());
            assert_eq!(name, "outputMonMute");
            assert_eq!(value, Value::Bool(true));
        }
        _ => panic!("wrong variant"),
    }
}

#[test]
fn set_master_param_other_emits_raw_name() {
    let l = layout();
    let bytes = Command::SetMasterParam {
        param: MasterParam::Other("masterMysteryKnob".to_string()),
        value: Value::Int(7),
    }
    .encode(&l)
    .unwrap();
    let frame = decode(&bytes[0]).unwrap();
    match frame {
        ChangeFrame::PropertyChanged { path, name, value } => {
            assert_eq!(path, l.master_channel_path().unwrap());
            assert_eq!(name, "masterMysteryKnob");
            assert_eq!(value, Value::Int(7));
        }
        _ => panic!("wrong variant"),
    }
}

#[test]
fn set_master_and_output_absent_node_return_missing_node_error() {
    // A layout from a tree with no MASTERCHANNEL/OUTPUT: encoding must error
    // with MissingNode, never panic or misroute.
    fn nc(name: &str, children: Vec<Node>) -> Node {
        Node {
            name: name.to_string(),
            properties: vec![],
            children,
        }
    }
    fn n(name: &str) -> Node {
        nc(name, vec![])
    }
    let phys = nc("PHYSICALINTERFACE", vec![n("FADER")]);
    let mut children = vec![phys, n("CHANNEL")];
    for _ in 0..13 {
        children.push(n("MIX"));
    }
    let l = Layout::from_full_sync(&nc("DEVICE", children)).unwrap();

    let err = Command::SetMasterParam {
        param: MasterParam::CompellorOn,
        value: Value::Bool(true),
    }
    .encode(&l)
    .unwrap_err();
    assert_eq!(
        err,
        EncodeError::MissingNode {
            what: "MASTERCHANNEL"
        }
    );

    let err = Command::SetOutputParam {
        param: OutputParam::MonMute,
        value: Value::Bool(true),
    }
    .encode(&l)
    .unwrap_err();
    assert_eq!(err, EncodeError::MissingNode { what: "OUTPUT" });
}

#[test]
fn set_ducker_recorder_player_address_their_singleton_paths() {
    let l = layout();

    let bytes = Command::SetDuckerParam {
        param: DuckerParam::Depth,
        value: Value::Double(-12.0),
    }
    .encode(&l)
    .unwrap();
    let frame = decode(&bytes[0]).unwrap();
    match frame {
        ChangeFrame::PropertyChanged { path, name, value } => {
            assert_eq!(path, l.ducker_path().unwrap());
            assert_eq!(name, "duckerDepth");
            assert_eq!(value, Value::Double(-12.0));
        }
        _ => panic!("wrong variant"),
    }

    // The record-transport command channel: requestRecordState starts/stops.
    let bytes = Command::SetRecorderParam {
        param: RecorderParam::RequestRecordState,
        value: Value::Int(1),
    }
    .encode(&l)
    .unwrap();
    let frame = decode(&bytes[0]).unwrap();
    match frame {
        ChangeFrame::PropertyChanged { path, name, value } => {
            assert_eq!(path, l.recorder_path().unwrap());
            assert_eq!(name, "requestRecordState");
            assert_eq!(value, Value::Int(1));
        }
        _ => panic!("wrong variant"),
    }

    let bytes = Command::SetPlayerParam {
        param: PlayerParam::Speed,
        value: Value::Int(0),
    }
    .encode(&l)
    .unwrap();
    let frame = decode(&bytes[0]).unwrap();
    match frame {
        ChangeFrame::PropertyChanged { path, name, value } => {
            assert_eq!(path, l.player_path().unwrap());
            assert_eq!(name, "playerSpeed");
            assert_eq!(value, Value::Int(0));
        }
        _ => panic!("wrong variant"),
    }
}

#[test]
fn set_headphone_param_addresses_indexed_path() {
    let l = layout();
    // Headphone index 1 -> the second HEADPHONE node.
    let bytes = Command::SetHeadphoneParam {
        headphone: 1,
        param: HeadphoneParam::Type,
        value: Value::Int(2),
    }
    .encode(&l)
    .unwrap();
    assert_eq!(bytes.len(), 1);
    let (path, name, value) = decode_single_prop(&bytes[0]);
    assert_eq!(path, l.headphone_path(1).unwrap());
    assert_eq!(name, "headphoneType");
    assert_eq!(value, Value::Int(2));
}

#[test]
fn set_headphone_param_out_of_range_index_returns_error_not_panic() {
    let l = layout();
    // The synthetic tree has 2 headphones; index 5 is past the run.
    let err = Command::SetHeadphoneParam {
        headphone: 5,
        param: HeadphoneParam::Colour,
        value: Value::String("ffffffff".into()),
    }
    .encode(&l)
    .unwrap_err();
    match err {
        EncodeError::OutOfRange { what, index, bound } => {
            assert_eq!(what, "headphone");
            assert_eq!(index, 5);
            assert_eq!(bound, 2);
        }
        _ => panic!("wrong error variant"),
    }
}

#[test]
fn set_effects_param_addresses_indexed_path() {
    let l = layout();
    // Effects slot index 2 -> the third EFFECTS_PARAMETERS node.
    let bytes = Command::SetEffectsParam {
        effects: 2,
        param: EffectsParam::ReverbMix,
        value: Value::Double(0.4),
    }
    .encode(&l)
    .unwrap();
    assert_eq!(bytes.len(), 1);
    let (path, name, value) = decode_single_prop(&bytes[0]);
    assert_eq!(path, l.effects_path(2).unwrap());
    assert_eq!(name, "reverbMix");
    assert_eq!(value, Value::Double(0.4));
}

#[test]
fn set_effects_param_out_of_range_index_returns_error_not_panic() {
    let l = layout();
    // The synthetic tree has 3 effects slots; index 7 is past the run.
    let err = Command::SetEffectsParam {
        effects: 7,
        param: EffectsParam::EchoMix,
        value: Value::Double(0.1),
    }
    .encode(&l)
    .unwrap_err();
    match err {
        EncodeError::OutOfRange { what, index, bound } => {
            assert_eq!(what, "effects");
            assert_eq!(index, 7);
            assert_eq!(bound, 3);
        }
        _ => panic!("wrong error variant"),
    }
}

#[test]
fn set_gui_param_addresses_gui_path() {
    let l = layout();
    // Display brightness, an Int, on the single GUI node.
    let bytes = Command::SetGuiParam {
        param: GuiParam::ScreenBrightness,
        value: Value::Int(250),
    }
    .encode(&l)
    .unwrap();
    assert_eq!(bytes.len(), 1);
    let (path, name, value) = decode_single_prop(&bytes[0]);
    assert_eq!(path, l.gui_path().unwrap());
    assert_eq!(name, "screenBrightness");
    assert_eq!(value, Value::Int(250));
}

#[test]
fn set_gui_param_other_emits_raw_name() {
    let l = layout();
    let bytes = Command::SetGuiParam {
        param: GuiParam::Other("mysteryGuiFlag".to_string()),
        value: Value::Bool(true),
    }
    .encode(&l)
    .unwrap();
    let (path, name, value) = decode_single_prop(&bytes[0]);
    assert_eq!(path, l.gui_path().unwrap());
    assert_eq!(name, "mysteryGuiFlag");
    assert_eq!(value, Value::Bool(true));
}

#[test]
fn set_system_param_addresses_system_path() {
    let l = layout();
    // A quirky-cased update flag, a Bool, on the single SYSTEM node.
    let bytes = Command::SetSystemParam {
        param: SystemParam::UpdateViaUsb,
        value: Value::Bool(true),
    }
    .encode(&l)
    .unwrap();
    assert_eq!(bytes.len(), 1);
    let (path, name, value) = decode_single_prop(&bytes[0]);
    assert_eq!(path, l.system_path().unwrap());
    // The variant's wire name keeps the firmware's USB casing verbatim.
    assert_eq!(name, "updateViaUSB");
    assert_eq!(value, Value::Bool(true));
}

#[test]
fn set_system_param_other_emits_raw_name() {
    let l = layout();
    let bytes = Command::SetSystemParam {
        param: SystemParam::Other("mysterySystemFlag".to_string()),
        value: Value::Int(7),
    }
    .encode(&l)
    .unwrap();
    let (path, name, value) = decode_single_prop(&bytes[0]);
    assert_eq!(path, l.system_path().unwrap());
    assert_eq!(name, "mysterySystemFlag");
    assert_eq!(value, Value::Int(7));
}

#[test]
fn set_pad_param_addresses_indexed_path() {
    let l = layout();
    // Pad index 2 -> the third PAD node inside SOUNDPADS (two-level path).
    let bytes = Command::SetPadParam {
        pad: 2,
        param: PadParam::ColourIndex,
        value: Value::Int(7),
    }
    .encode(&l)
    .unwrap();
    assert_eq!(bytes.len(), 1);
    let (path, name, value) = decode_single_prop(&bytes[0]);
    assert_eq!(path, l.pad_path(2).unwrap());
    assert_eq!(name, "padColourIndex");
    assert_eq!(value, Value::Int(7));
}

#[test]
fn set_pad_param_out_of_range_index_returns_error_not_panic() {
    let l = layout();
    // The synthetic tree has 3 pads; index 9 is past the run.
    let err = Command::SetPadParam {
        pad: 9,
        param: PadParam::Gain,
        value: Value::Double(0.5),
    }
    .encode(&l)
    .unwrap_err();
    match err {
        EncodeError::OutOfRange { what, index, bound } => {
            assert_eq!(what, "pad");
            assert_eq!(index, 9);
            assert_eq!(bound, 3);
        }
        _ => panic!("wrong error variant"),
    }
}

#[test]
fn set_pad_param_other_emits_raw_name() {
    let l = layout();
    let bytes = Command::SetPadParam {
        pad: 0,
        param: PadParam::Other("padMysteryFlag".to_string()),
        value: Value::Bool(true),
    }
    .encode(&l)
    .unwrap();
    let (path, name, value) = decode_single_prop(&bytes[0]);
    assert_eq!(path, l.pad_path(0).unwrap());
    assert_eq!(name, "padMysteryFlag");
    assert_eq!(value, Value::Bool(true));
}

#[test]
fn set_gui_param_absent_node_returns_missing_node_error() {
    let l = minimal_layout();

    assert_eq!(
        Command::SetGuiParam {
            param: GuiParam::ScreenBrightness,
            value: Value::Int(250),
        }
        .encode(&l)
        .unwrap_err(),
        EncodeError::MissingNode { what: "GUI" }
    );
}

#[test]
fn set_system_param_absent_node_returns_missing_node_error() {
    let l = minimal_layout();

    assert_eq!(
        Command::SetSystemParam {
            param: SystemParam::PowerOffRequest,
            value: Value::Bool(true),
        }
        .encode(&l)
        .unwrap_err(),
        EncodeError::MissingNode { what: "SYSTEM" }
    );
}

#[test]
fn set_ducker_recorder_player_absent_node_return_missing_node_error() {
    let l = minimal_layout();

    assert_eq!(
        Command::SetDuckerParam {
            param: DuckerParam::Depth,
            value: Value::Double(-9.0),
        }
        .encode(&l)
        .unwrap_err(),
        EncodeError::MissingNode { what: "DUCKER" }
    );
    assert_eq!(
        Command::SetRecorderParam {
            param: RecorderParam::RecordState,
            value: Value::Int(0),
        }
        .encode(&l)
        .unwrap_err(),
        EncodeError::MissingNode { what: "RECORDER" }
    );
    assert_eq!(
        Command::SetPlayerParam {
            param: PlayerParam::State,
            value: Value::Int(0),
        }
        .encode(&l)
        .unwrap_err(),
        EncodeError::MissingNode { what: "PLAYER" }
    );
}

#[test]
fn set_channel_param_out_of_range_fader_returns_error_not_panic() {
    let l = layout();
    // Virtual1 (index 6) resolves past the synthetic layout's 3 channels.
    let err = Command::SetChannelParam {
        fader: Fader::Virtual1,
        param: ChannelParam::EqOn,
        value: Value::Bool(true),
    }
    .encode(&l)
    .unwrap_err();
    match err {
        EncodeError::OutOfRange { what, index, .. } => {
            assert_eq!(what, "fader");
            assert_eq!(index, 6);
        }
        _ => panic!("wrong error variant"),
    }
}

#[test]
fn out_of_range_fader_returns_error_not_panic() {
    let l = layout();
    // Virtual1 is a valid Pro II strip (index 6) but the synthetic layout
    // only has 3 channels, so it resolves past the discovered count.
    let err = Command::SetFaderMute {
        fader: Fader::Virtual1,
        mute: true,
    }
    .encode(&l)
    .unwrap_err();
    match err {
        EncodeError::OutOfRange { what, index, .. } => {
            assert_eq!(what, "fader");
            assert_eq!(index, 6);
        }
        _ => panic!("wrong error variant"),
    }
}

#[test]
fn fader_not_on_model_returns_error_not_panic() {
    let l = layout(); // Pro II (empty SYSTEM node -> default)
                      // Virtual4 only exists on the Duo.
    let err = Command::SetFaderMute {
        fader: Fader::Virtual4,
        mute: true,
    }
    .encode(&l)
    .unwrap_err();
    match err {
        EncodeError::FaderNotOnModel { fader, model } => {
            assert_eq!(fader, Fader::Virtual4);
            assert_eq!(model, DeviceModel::Pro2);
        }
        _ => panic!("wrong error variant"),
    }
}

#[test]
fn out_of_range_mix_cell_returns_error_not_panic() {
    let l = layout();
    // CallMe1 (protocol source 16) sits past the matrix on a Pro II layout.
    let err = Command::SetMixDisabled {
        source: Source::CallMe1,
        mix: MixOutput::Headphone1,
        disabled: true,
    }
    .encode(&l)
    .unwrap_err();
    match err {
        EncodeError::MixCellOutOfRange { source, .. } => assert_eq!(source, 16),
        _ => panic!("wrong error variant"),
    }
}

/// Critical: the same Command encodes differently when the Layout's bases
/// differ. Proves Command::encode is layout-driven, not constant-driven.
#[test]
fn encoding_depends_on_layout_not_constants() {
    fn n(name: &str) -> Node {
        Node {
            name: name.to_string(),
            properties: vec![],
            children: vec![],
        }
    }
    fn nc(name: &str, children: Vec<Node>) -> Node {
        Node {
            name: name.to_string(),
            properties: vec![],
            children,
        }
    }

    // Tree A: CHANNEL at root index 3.
    let phys = nc("PHYSICALINTERFACE", vec![n("FADER"), n("FADER")]);
    let mut a_children = vec![phys.clone(), n("X"), n("Y"), n("CHANNEL"), n("CHANNEL")];
    for _ in 0..13 {
        a_children.push(n("MIX"));
    }
    let layout_a = Layout::from_full_sync(&nc("DEVICE", a_children)).unwrap();

    // Tree B: CHANNEL at root index 5 (more leading siblings).
    let mut b_children = vec![
        phys,
        n("X"),
        n("Y"),
        n("Z"),
        n("W"),
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

    // Same Command, but DIFFERENT wire paths -> different bytes.
    assert_ne!(
        a, b,
        "Command bytes must reflect Layout differences (proves no hardcoded base)"
    );

    // Decode each, verify they address the right CHANNEL in their tree.
    let fa = decode(&a[0]).unwrap();
    let fb = decode(&b[0]).unwrap();
    let path_a = match fa {
        ChangeFrame::PropertyChanged { path, .. } => path,
        _ => panic!(),
    };
    let path_b = match fb {
        ChangeFrame::PropertyChanged { path, .. } => path,
        _ => panic!(),
    };
    assert_eq!(path_a, vec![3]);
    assert_eq!(path_b, vec![5]);
}

// --- Frozen wire-byte goldens for the layout-independent / special-path
// commands. The var-value byte sequences (Bool, Binary) are the exact
// frames pinned by `juce_var::tests::write_matches_known_juce_frames`.

#[test]
fn screen_touched_golden_bytes() {
    let bytes = Command::ScreenTouched.encode(&layout()).unwrap();
    assert_eq!(bytes.len(), 1);
    // Header (changeType + nLevels=1 + path num-bytes prefix) then the
    // name with NO path value and NO var value. Not a clean propertyChanged.
    let mut expected = vec![0x01, 0x01, 0x01, 0x01];
    expected.extend_from_slice(b"screenTouched\0");
    assert_eq!(bytes[0], expected);
}

#[test]
fn power_off_golden_bytes() {
    let l = layout();
    let sys_idx = l.system_path().unwrap()[0] as u8;
    let bytes = Command::PowerOff.encode(&l).unwrap();
    assert_eq!(bytes.len(), 1);
    // propertyChanged, path=[system_path], "powerOffRequest", var Bool(true).
    let mut expected = vec![0x01, 0x01, 0x01, 0x01, sys_idx];
    expected.extend_from_slice(b"powerOffRequest\0");
    expected.extend_from_slice(&[0x01, 0x01, 0x02]); // var Bool(true)
    assert_eq!(bytes[0], expected);
}

#[test]
fn link_callme_golden_bytes() {
    // CallMe1 = protocol source 16, Headphone1 = protocol mix 0.
    let bytes = Command::LinkCallMe {
        source: Source::CallMe1,
        mix: MixOutput::Headphone1,
    }
    .encode(&layout())
    .unwrap();
    assert_eq!(bytes.len(), 1);
    // path[0] = (16<<8)|(4+0) = 4100 -> 2-byte compint `02 04 10`.
    let mut expected = vec![0x01, 0x01, 0x01, 0x02, 0x04, 0x10];
    expected.extend_from_slice(b"mixLinkRequest\0");
    expected.extend_from_slice(&[0x01, 0x07, 0x08, 0x01, 0x01, 0x02, 0x01, 0x01, 0x02]); // var Binary blob
    assert_eq!(bytes[0], expected);
}

#[test]
fn unlink_callme_golden_bytes() {
    // CallMe2 = protocol source 17, Headphone3 = protocol mix 2.
    let bytes = Command::UnlinkCallMe {
        source: Source::CallMe2,
        mix: MixOutput::Headphone3,
    }
    .encode(&layout())
    .unwrap();
    assert_eq!(bytes.len(), 1);
    // path[0] = (17<<8)|(4+2) = 4358 -> 2-byte compint `02 06 11`.
    let mut expected = vec![0x01, 0x01, 0x01, 0x02, 0x06, 0x11];
    expected.extend_from_slice(b"mixUnlinkRequest\0");
    expected.extend_from_slice(&[0x01, 0x07, 0x08, 0x01, 0x01, 0x02, 0x01, 0x01, 0x02]); // var Binary blob
    assert_eq!(bytes[0], expected);
}

#[test]
fn set_remaining_param_families_encode_cleanly() {
    fn n(name: &str) -> Node {
        Node {
            name: name.to_string(),
            properties: vec![],
            children: vec![],
        }
    }
    fn nc(name: &str, children: Vec<Node>) -> Node {
        Node {
            name: name.to_string(),
            properties: vec![],
            children,
        }
    }

    let phys = nc("PHYSICALINTERFACE", vec![n("FADER")]);
    let mut children = vec![phys, n("CHANNEL")];
    for _ in 0..13 {
        children.push(n("MIX"));
    }
    children.extend(vec![
        n("NETWORK"),
        n("AUDIO"),
        n("BUILD"),
        n("APP"),
        n("THEME"),
        n("CURRENTSHOW"),
        n("SHOWCONTROL"),
        n("RECORDINGS"),
        n("RADIO"),
        nc("SHOWS", vec![nc("SHOW", vec![])]),
        nc("RECORDINGS", vec![nc("RECORDING", vec![])]),
        n("STORAGEVOLUME"),
        n("RADIOTX"),
        n("RADIORX"),
        n("WIFISCANRESULT"),
        n("STREAMERXMIXPRESET"),
        n("STREAMERXSTREAMMIX"),
        n("RCSYNCMIX"),
        n("MIXMINUSES"),
    ]);
    let l = Layout::from_full_sync(&nc("DEVICE", children)).unwrap();

    let cmd_network = Command::SetNetworkParam {
        param: NetworkParam::BtVisible,
        value: Value::Bool(true),
    };
    assert_eq!(cmd_network.encode(&l).unwrap().len(), 1);

    let cmd_show_control = Command::SetShowControlParam {
        param: ShowControlParam::NewFromDefaultMuted,
        value: Value::Bool(true),
    };
    assert_eq!(cmd_show_control.encode(&l).unwrap().len(), 1);

    let cmd_show = Command::SetShowParam {
        show: 0,
        param: ShowParam::Name,
        value: Value::String("Test".to_string()),
    };
    assert_eq!(cmd_show.encode(&l).unwrap().len(), 1);
}
