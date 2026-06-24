use super::*;
use crate::valuetree::Property;

fn node(name: &str) -> Node {
    Node {
        name: name.to_string(),
        properties: vec![],
        children: vec![],
    }
}
fn node_with_children(name: &str, children: Vec<Node>) -> Node {
    Node {
        name: name.to_string(),
        properties: vec![],
        children,
    }
}

/// Build a synthetic tree where PHYSICALINTERFACE / CHANNEL / MIX are at
/// **non-default positions** so the test fails if anything is hardcoded
/// to 0x1C (28), 62, or 0x04. Layout values come from tree positions,
/// not constants.
fn synthetic_tree() -> Node {
    let phys = node_with_children(
        "PHYSICALINTERFACE",
        vec![
            node("HEADER"), // index 0
            node("FADER"),  // index 1 (first_fader_in_phys = 1, NOT 4)
            node("FADER"),  // index 2
            node("FADER"),  // index 3
            node("FOOTER"), // breaks FADER sequence
        ],
    );

    // root.children deliberately puts CHANNEL at 4 (NOT 28) and MIX at 8
    // (NOT 62), with 2 sources => mix_total = 26 (= 2 * 13).
    let mut children = vec![
        node("OTHER1"),  // 0
        node("OTHER2"),  // 1
        phys,            // 2: physical_interface_idx = 2 (NOT 0)
        node("OTHER3"),  // 3
        node("CHANNEL"), // 4: first_channel = 4 (NOT 28)
        node("CHANNEL"), // 5
        node("CHANNEL"), // 6
        node("OTHER4"),  // 7: breaks CHANNEL run
    ];
    // first_mix = 8 (NOT 62); 26 MIX nodes -> source_count = 2
    for _ in 0..26 {
        children.push(node("MIX"));
    }
    // 4 INPUTSOURCE nodes appended after MIX (first_input_source = 34), then
    // a non-INPUTSOURCE node so the run length is exactly 4 (a second run on
    // a real device would be separated like this).
    for _ in 0..4 {
        children.push(node("INPUTSOURCE")); // 34..=37
    }
    // Singleton families after the INPUTSOURCE run (the MASTERCHANNEL at 38
    // also breaks that run at exactly 4).
    children.push(node("MASTERCHANNEL")); // 38
    children.push(node("OUTPUT")); // 39
    children.push(node("DUCKER")); // 40
    children.push(node("RECORDER")); // 41
    children.push(node("PLAYER")); // 42
                                   // HEADPHONE is multi-instance; two at the tail so the run length is 2.
    children.push(node("HEADPHONE")); // 43: first_headphone = 43
    children.push(node("HEADPHONE")); // 44
                                      // EFFECTS_PARAMETERS is multi-instance; a run of three so the
                                      // length is 3 and the run start (45) is NOT a round constant.
    children.push(node("EFFECTS_PARAMETERS")); // 45: first_effects = 45
    children.push(node("EFFECTS_PARAMETERS")); // 46
    children.push(node("EFFECTS_PARAMETERS")); // 47
    children.push(node("GUI")); // 48: gui singleton after the effects run
                                // SOUNDPADS container at 49; PAD children form a run of 3 inside it,
                                // with a non-PAD child first so first_pad (1) is NOT a round constant.
    children.push(node_with_children(
        "SOUNDPADS", // 49: soundpads_idx
        vec![
            node("PADHEADER"), // child 0 (breaks PAD start off zero)
            node("PAD"),       // child 1: first_pad = 1
            node("PAD"),       // child 2
            node("PAD"),       // child 3
        ],
    ));
    children.push(node("SYSTEM")); // 50: system singleton at the tail
    node_with_children("DEVICE", children)
}

#[test]
fn discovers_bases_from_tree_positions_not_constants() {
    let root = synthetic_tree();
    let layout = Layout::from_full_sync(&root).expect("builds");

    // Critical: these are tree positions, not the 1.7.3-firmware constants.
    assert_eq!(layout.physical_interface_idx(), 2);
    assert_eq!(layout.first_fader_in_phys(), 1);
    assert_eq!(layout.fader_count(), 3);
    assert_eq!(layout.first_channel(), 4);
    assert_eq!(layout.channel_count(), 3);
    assert_eq!(layout.first_mix(), 8);
    assert_eq!(layout.source_count(), 2);
    assert_eq!(layout.mix_count_per_source(), 13);
}

#[test]
fn channel_path_uses_discovered_first_channel() {
    let layout = Layout::from_full_sync(&synthetic_tree()).unwrap();
    // first_channel = 4 in the synthetic tree, so channel 0 lives at [4].
    assert_eq!(layout.channel_path(0), Some(vec![4]));
    assert_eq!(layout.channel_path(2), Some(vec![6]));
    // Out of range.
    assert_eq!(layout.channel_path(3), None);
    // And the inverse.
    assert_eq!(layout.channel_index_from_path(&[4]), Some(0));
    assert_eq!(layout.channel_index_from_path(&[6]), Some(2));
    assert_eq!(layout.channel_index_from_path(&[7]), None);
    assert_eq!(layout.channel_index_from_path(&[3]), None);
    assert_eq!(layout.channel_index_from_path(&[4, 0]), None);
}

#[test]
fn fader_path_is_two_level_through_physical_interface() {
    let layout = Layout::from_full_sync(&synthetic_tree()).unwrap();
    // [physical_interface_idx=2, first_fader_in_phys=1 + n]
    assert_eq!(layout.fader_path(0), Some(vec![2, 1]));
    assert_eq!(layout.fader_path(2), Some(vec![2, 3]));
    assert_eq!(layout.fader_path(3), None);
    // Inverse.
    assert_eq!(layout.fader_index_from_path(&[2, 1]), Some(0));
    assert_eq!(layout.fader_index_from_path(&[2, 3]), Some(2));
    assert_eq!(layout.fader_index_from_path(&[2, 4]), None);
    assert_eq!(layout.fader_index_from_path(&[0, 1]), None); // wrong parent
    assert_eq!(layout.fader_index_from_path(&[1]), None); // wrong length
}

#[test]
fn mix_cell_path_is_source_major() {
    let layout = Layout::from_full_sync(&synthetic_tree()).unwrap();
    // first_mix = 8, 13 mixes per source.
    assert_eq!(layout.mix_cell_path(0, 0), Some(vec![8]));
    assert_eq!(layout.mix_cell_path(0, 12), Some(vec![20]));
    assert_eq!(layout.mix_cell_path(1, 0), Some(vec![21]));
    assert_eq!(layout.mix_cell_path(1, 12), Some(vec![33]));
    assert_eq!(layout.mix_cell_path(2, 0), None); // source out of range
    assert_eq!(layout.mix_cell_path(0, 13), None); // mix out of range
                                                   // Inverse.
    assert_eq!(layout.mix_cell_from_path(&[8]), Some((0, 0)));
    assert_eq!(layout.mix_cell_from_path(&[20]), Some((0, 12)));
    assert_eq!(layout.mix_cell_from_path(&[21]), Some((1, 0)));
    assert_eq!(layout.mix_cell_from_path(&[33]), Some((1, 12)));
    assert_eq!(layout.mix_cell_from_path(&[34]), None); // past matrix
    assert_eq!(layout.mix_cell_from_path(&[7]), None); // before first_mix
}

#[test]
fn input_source_path_uses_discovered_first_input_source() {
    let layout = Layout::from_full_sync(&synthetic_tree()).unwrap();
    // first_input_source = 34, run length 4.
    assert_eq!(layout.first_input_source(), Some(34));
    assert_eq!(layout.input_source_count(), 4);
    assert_eq!(layout.input_source_path(0), Some(vec![34]));
    assert_eq!(layout.input_source_path(3), Some(vec![37]));
    assert_eq!(layout.input_source_path(4), None); // past the run
                                                   // Inverse.
    assert_eq!(layout.input_source_index_from_path(&[34]), Some(0));
    assert_eq!(layout.input_source_index_from_path(&[37]), Some(3));
    assert_eq!(layout.input_source_index_from_path(&[38]), None); // MASTERCHANNEL
    assert_eq!(layout.input_source_index_from_path(&[33]), None); // last MIX
    assert_eq!(layout.input_source_index_from_path(&[34, 0]), None); // wrong length
}

#[test]
fn master_and_output_paths_use_discovered_singletons() {
    let layout = Layout::from_full_sync(&synthetic_tree()).unwrap();
    // first_input_source=34, run 4 -> MASTERCHANNEL at 38, OUTPUT at 39.
    assert_eq!(layout.master_channel(), Some(38));
    assert_eq!(layout.output(), Some(39));
    assert_eq!(layout.master_channel_path(), Some(vec![38]));
    assert_eq!(layout.output_path(), Some(vec![39]));
    // Inverse predicates.
    assert!(layout.is_master_channel_path(&[38]));
    assert!(!layout.is_master_channel_path(&[39])); // that's OUTPUT
    assert!(!layout.is_master_channel_path(&[38, 0])); // wrong length
    assert!(layout.is_output_path(&[39]));
    assert!(!layout.is_output_path(&[38])); // that's MASTERCHANNEL
}

#[test]
fn master_and_output_absent_yield_none_not_panic() {
    // A minimal tree with no MASTERCHANNEL/OUTPUT still builds.
    let phys = node_with_children("PHYSICALINTERFACE", vec![node("FADER")]);
    let mut children = vec![phys, node("CHANNEL")];
    for _ in 0..13 {
        children.push(node("MIX"));
    }
    let layout = Layout::from_full_sync(&node_with_children("DEVICE", children)).unwrap();
    assert_eq!(layout.master_channel(), None);
    assert_eq!(layout.output(), None);
    assert_eq!(layout.master_channel_path(), None);
    assert_eq!(layout.output_path(), None);
    assert!(!layout.is_master_channel_path(&[0]));
    assert!(!layout.is_output_path(&[0]));
}

#[test]
fn ducker_recorder_player_paths_use_discovered_singletons() {
    let layout = Layout::from_full_sync(&synthetic_tree()).unwrap();
    // DUCKER at 40, RECORDER at 41, PLAYER at 42 in the synthetic tree.
    assert_eq!(layout.ducker(), Some(40));
    assert_eq!(layout.recorder(), Some(41));
    assert_eq!(layout.player(), Some(42));
    assert_eq!(layout.ducker_path(), Some(vec![40]));
    assert_eq!(layout.recorder_path(), Some(vec![41]));
    assert_eq!(layout.player_path(), Some(vec![42]));
    // Each predicate accepts only its own node, rejects the neighbours.
    assert!(layout.is_ducker_path(&[40]));
    assert!(!layout.is_ducker_path(&[41]));
    assert!(layout.is_recorder_path(&[41]));
    assert!(!layout.is_recorder_path(&[42]));
    assert!(layout.is_player_path(&[42]));
    assert!(!layout.is_player_path(&[40]));
    // Wrong length never matches.
    assert!(!layout.is_player_path(&[42, 0]));
}

#[test]
fn headphone_paths_use_discovered_run() {
    let layout = Layout::from_full_sync(&synthetic_tree()).unwrap();
    // first_headphone = 43, run length 2.
    assert_eq!(layout.first_headphone(), Some(43));
    assert_eq!(layout.headphone_count(), 2);
    assert_eq!(layout.headphone_path(0), Some(vec![43]));
    assert_eq!(layout.headphone_path(1), Some(vec![44]));
    assert_eq!(layout.headphone_path(2), None); // past the run
                                                // Inverse.
    assert_eq!(layout.headphone_index_from_path(&[43]), Some(0));
    assert_eq!(layout.headphone_index_from_path(&[44]), Some(1));
    assert_eq!(layout.headphone_index_from_path(&[45]), None); // past the run
    assert_eq!(layout.headphone_index_from_path(&[42]), None); // PLAYER, before run
    assert_eq!(layout.headphone_index_from_path(&[43, 0]), None); // wrong length
}

#[test]
fn effects_paths_use_discovered_run() {
    let layout = Layout::from_full_sync(&synthetic_tree()).unwrap();
    // first_effects = 45, run length 3.
    assert_eq!(layout.first_effects(), Some(45));
    assert_eq!(layout.effects_count(), 3);
    assert_eq!(layout.effects_path(0), Some(vec![45]));
    assert_eq!(layout.effects_path(1), Some(vec![46]));
    assert_eq!(layout.effects_path(2), Some(vec![47]));
    assert_eq!(layout.effects_path(3), None); // past the run
                                              // Inverse.
    assert_eq!(layout.effects_index_from_path(&[45]), Some(0));
    assert_eq!(layout.effects_index_from_path(&[47]), Some(2));
    assert_eq!(layout.effects_index_from_path(&[48]), None); // GUI, past the run
    assert_eq!(layout.effects_index_from_path(&[44]), None); // HEADPHONE, before run
    assert_eq!(layout.effects_index_from_path(&[45, 0]), None); // wrong length
}

#[test]
fn gui_path_uses_discovered_singleton() {
    let layout = Layout::from_full_sync(&synthetic_tree()).unwrap();
    // GUI at 48 in the synthetic tree (after the effects run).
    assert_eq!(layout.gui(), Some(48));
    assert_eq!(layout.gui_path(), Some(vec![48]));
    assert!(layout.is_gui_path(&[48]));
    assert!(!layout.is_gui_path(&[47])); // that's the last EFFECTS slot
    assert!(!layout.is_gui_path(&[48, 0])); // wrong length
}

#[test]
fn system_path_uses_discovered_singleton() {
    let layout = Layout::from_full_sync(&synthetic_tree()).unwrap();
    // SYSTEM at 50 in the synthetic tree (after the SOUNDPADS container at 49).
    assert_eq!(layout.system(), Some(50));
    assert_eq!(layout.system_path(), Some(vec![50]));
    assert!(layout.is_system_path(&[50]));
    assert!(!layout.is_system_path(&[49])); // that's SOUNDPADS
    assert!(!layout.is_system_path(&[50, 0])); // wrong length
}

#[test]
fn pad_path_is_two_level_through_soundpads() {
    let layout = Layout::from_full_sync(&synthetic_tree()).unwrap();
    // SOUNDPADS at 49, first_pad = 1 (PADHEADER at child 0), 3 pads.
    assert_eq!(layout.soundpads(), Some(49));
    assert_eq!(layout.first_pad(), 1);
    assert_eq!(layout.pad_count(), 3);
    // [soundpads_idx=49, first_pad=1 + n]
    assert_eq!(layout.pad_path(0), Some(vec![49, 1]));
    assert_eq!(layout.pad_path(2), Some(vec![49, 3]));
    assert_eq!(layout.pad_path(3), None);

    assert_eq!(layout.pad_index_from_path(&[49, 1]), Some(0));
    assert_eq!(layout.pad_index_from_path(&[49, 3]), Some(2));
    assert_eq!(layout.pad_index_from_path(&[49, 4]), None); // past the run
    assert_eq!(layout.pad_index_from_path(&[49, 0]), None); // that's PADHEADER
    assert_eq!(layout.pad_index_from_path(&[2, 1]), None); // wrong parent (PHYS)
    assert_eq!(layout.pad_index_from_path(&[49]), None); // wrong length
}

#[test]
fn ducker_recorder_player_headphone_absent_yield_none_not_panic() {
    // A minimal tree with none of the new families still builds.
    let phys = node_with_children("PHYSICALINTERFACE", vec![node("FADER")]);
    let mut children = vec![phys, node("CHANNEL")];
    for _ in 0..13 {
        children.push(node("MIX"));
    }
    let layout = Layout::from_full_sync(&node_with_children("DEVICE", children)).unwrap();
    assert_eq!(layout.ducker(), None);
    assert_eq!(layout.recorder(), None);
    assert_eq!(layout.player(), None);
    assert_eq!(layout.ducker_path(), None);
    assert_eq!(layout.recorder_path(), None);
    assert_eq!(layout.player_path(), None);
    assert!(!layout.is_ducker_path(&[0]));
    assert!(!layout.is_recorder_path(&[0]));
    assert!(!layout.is_player_path(&[0]));
    assert_eq!(layout.first_headphone(), None);
    assert_eq!(layout.headphone_count(), 0);
    assert_eq!(layout.headphone_path(0), None);
    assert_eq!(layout.headphone_index_from_path(&[0]), None);
    assert_eq!(layout.first_effects(), None);
    assert_eq!(layout.effects_count(), 0);
    assert_eq!(layout.effects_path(0), None);
    assert_eq!(layout.effects_index_from_path(&[0]), None);
    assert_eq!(layout.gui(), None);
    assert_eq!(layout.gui_path(), None);
    assert!(!layout.is_gui_path(&[0]));
    assert_eq!(layout.soundpads(), None);
    assert_eq!(layout.pad_count(), 0);
    assert_eq!(layout.pad_path(0), None);
    assert_eq!(layout.pad_index_from_path(&[0, 0]), None);
    assert_eq!(layout.system(), None);
    assert_eq!(layout.system_path(), None);
    assert!(!layout.is_system_path(&[0]));
}

#[test]
fn input_source_absent_yields_none_not_panic() {
    // A tree with no INPUTSOURCE still builds; addressing just returns None.
    let phys = node_with_children("PHYSICALINTERFACE", vec![node("FADER")]);
    let mut children = vec![phys, node("CHANNEL")];
    for _ in 0..13 {
        children.push(node("MIX"));
    }
    let layout = Layout::from_full_sync(&node_with_children("DEVICE", children)).unwrap();
    assert_eq!(layout.first_input_source(), None);
    assert_eq!(layout.input_source_count(), 0);
    assert_eq!(layout.input_source_path(0), None);
    assert_eq!(layout.input_source_index_from_path(&[0]), None);
}

#[test]
fn fails_when_physical_interface_missing() {
    let root = node_with_children("DEVICE", vec![node("CHANNEL"), node("MIX")]);
    let err = Layout::from_full_sync(&root).unwrap_err();
    assert_eq!(err, BuildError::MissingNode("PHYSICALINTERFACE under root"));
}

#[test]
fn fails_when_fader_missing_inside_physical_interface() {
    let phys = node_with_children("PHYSICALINTERFACE", vec![node("HEADER")]);
    let mut children = vec![phys, node("CHANNEL")];
    for _ in 0..13 {
        children.push(node("MIX"));
    }
    let root = node_with_children("DEVICE", children);
    let err = Layout::from_full_sync(&root).unwrap_err();
    assert_eq!(
        err,
        BuildError::MissingNode("FADER under PHYSICALINTERFACE")
    );
}

#[test]
fn fails_when_channel_missing() {
    let phys = node_with_children("PHYSICALINTERFACE", vec![node("FADER")]);
    let mut children = vec![phys];
    for _ in 0..13 {
        children.push(node("MIX"));
    }
    let root = node_with_children("DEVICE", children);
    let err = Layout::from_full_sync(&root).unwrap_err();
    assert_eq!(err, BuildError::MissingNode("CHANNEL under root"));
}

#[test]
fn fails_when_mix_missing() {
    let phys = node_with_children("PHYSICALINTERFACE", vec![node("FADER")]);
    let root = node_with_children("DEVICE", vec![phys, node("CHANNEL")]);
    let err = Layout::from_full_sync(&root).unwrap_err();
    assert_eq!(err, BuildError::MissingNode("MIX under root"));
}

#[test]
fn fails_when_mix_count_not_divisible_by_matrix_width() {
    let phys = node_with_children("PHYSICALINTERFACE", vec![node("FADER")]);
    let mut children = vec![phys, node("CHANNEL")];
    // 14 MIX nodes: not a clean multiple of 13.
    for _ in 0..14 {
        children.push(node("MIX"));
    }
    let root = node_with_children("DEVICE", children);
    let err = Layout::from_full_sync(&root).unwrap_err();
    match err {
        BuildError::ShapeMismatch { what, .. } => assert_eq!(what, "MIX"),
        other => panic!("expected ShapeMismatch, got {other:?}"),
    }
}

/// Build a SYSTEM node carrying the given board type, prepended to an
/// otherwise-valid synthetic tree so `from_full_sync` succeeds.
fn tree_with_system(props: Vec<Property>) -> Node {
    let phys = node_with_children("PHYSICALINTERFACE", vec![node("FADER")]);
    let system = Node {
        name: "SYSTEM".to_string(),
        properties: props,
        children: vec![],
    };
    let mut children = vec![system, phys, node("CHANNEL")];
    for _ in 0..13 {
        children.push(node("MIX"));
    }
    node_with_children("DEVICE", children)
}

#[test]
fn model_defaults_to_pro2_without_system_node() {
    // A minimal tree with no SYSTEM node at all: model detection defaults to
    // Pro II. (The main synthetic_tree carries an empty SYSTEM node for the
    // singleton-addressing tests, so it can't prove the absent-node default.)
    let phys = node_with_children("PHYSICALINTERFACE", vec![node("FADER")]);
    let mut children = vec![phys, node("CHANNEL")];
    for _ in 0..13 {
        children.push(node("MIX"));
    }
    let layout = Layout::from_full_sync(&node_with_children("DEVICE", children)).unwrap();
    assert_eq!(layout.system(), None);
    assert_eq!(layout.model(), DeviceModel::Pro2);
}

#[test]
fn model_reads_board_type_from_system() {
    let pro2 = tree_with_system(vec![Property {
        name: "boardType".to_string(),
        value: Value::Int(0),
    }]);
    assert_eq!(
        Layout::from_full_sync(&pro2).unwrap().model(),
        DeviceModel::Pro2
    );

    let duo = tree_with_system(vec![Property {
        name: "boardType".to_string(),
        value: Value::Int(1),
    }]);
    assert_eq!(
        Layout::from_full_sync(&duo).unwrap().model(),
        DeviceModel::Duo
    );
}

#[test]
fn model_falls_back_to_system_name() {
    // No boardType -> systemName decides.
    let duo = tree_with_system(vec![Property {
        name: "systemName".to_string(),
        value: Value::String("RODECaster Duo".to_string()),
    }]);
    assert_eq!(
        Layout::from_full_sync(&duo).unwrap().model(),
        DeviceModel::Duo
    );
}
